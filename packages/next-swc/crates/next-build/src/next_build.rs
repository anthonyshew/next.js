use std::{
    collections::{HashMap, HashSet},
    env::current_dir,
    path::{PathBuf, MAIN_SEPARATOR},
};

use anyhow::{anyhow, bail, Context, Result};
use dunce::canonicalize;
use next_core::{
    self,
    mode::NextMode,
    next_client::{get_client_chunking_context, get_client_compile_time_info},
    next_client_reference::{ClientReferenceType, ClientReferencesByEntryVc},
    next_config::load_next_config,
    next_dynamic::NextDynamicEntriesVc,
    next_server::{get_server_chunking_context, get_server_compile_time_info},
    url_node::get_sorted_routes,
};
use serde::Serialize;
use turbo_tasks::{
    graph::{AdjacencyMap, GraphTraversal},
    CollectiblesSource, CompletionVc, CompletionsVc, RawVc, TransientInstance, TransientValue,
    TryJoinIterExt,
};
use turbopack_binding::{
    turbo::tasks_fs::{
        rebase, DiskFileSystemVc, FileContent, FileSystem, FileSystemPath, FileSystemPathVc,
        FileSystemVc,
    },
    turbopack::{
        build::BuildChunkingContextVc,
        cli_utils::issue::{ConsoleUiVc, LogOptions},
        core::{
            asset::{Asset, AssetVc, AssetsVc},
            chunk::ChunkingContext,
            environment::ServerAddrVc,
            issue::{IssueReporter, IssueReporterVc, IssueSeverity, IssueVc},
            output::{OutputAssetVc, OutputAssetsVc},
            reference::AssetReference,
            virtual_fs::VirtualFileSystemVc,
        },
        dev::DevChunkingContextVc,
        ecmascript::utils::StringifyJs,
        env::dotenv::load_env,
        node::execution_context::ExecutionContextVc,
        turbopack::evaluate_context::node_build_environment,
    },
};

use crate::{
    build_options::{BuildContext, BuildOptions},
    manifests::{
        AppBuildManifest, AppPathsManifest, BuildManifest, ClientBuildManifest, FontManifest,
        MiddlewaresManifest, NextFontManifest, PagesManifest, ReactLoadableManifest,
        ServerReferenceManifest,
    },
    next_app::{
        app_client_reference::compute_app_client_references_chunks,
        app_entries::{compute_app_entries_chunks, get_app_entries},
    },
    next_pages::page_entries::{compute_page_entries_chunks, get_page_entries},
};

#[turbo_tasks::function]
pub(crate) async fn next_build(options: TransientInstance<BuildOptions>) -> Result<CompletionVc> {
    let project_root = options
        .dir
        .as_ref()
        .map(canonicalize)
        .unwrap_or_else(current_dir)
        .context("project directory can't be found")?
        .to_str()
        .context("project directory contains invalid characters")?
        .to_string();

    let workspace_root = if let Some(root) = options.root.as_ref() {
        canonicalize(root)
            .context("root directory can't be found")?
            .to_str()
            .context("root directory contains invalid characters")?
            .to_string()
    } else {
        project_root.clone()
    };

    let browserslist_query = "last 1 Chrome versions, last 1 Firefox versions, last 1 Safari \
                              versions, last 1 Edge versions";

    let log_options = LogOptions {
        project_dir: PathBuf::from(project_root.clone()),
        current_dir: current_dir().unwrap(),
        show_all: options.show_all,
        log_detail: options.log_detail,
        log_level: options.log_level.unwrap_or(IssueSeverity::Warning),
    };

    let issue_reporter: IssueReporterVc =
        ConsoleUiVc::new(TransientInstance::new(log_options)).into();
    let node_fs = node_fs(&project_root, issue_reporter);
    let node_root = node_fs.root().join(".next");
    let client_fs = client_fs(&project_root, issue_reporter);
    let client_root = client_fs.root().join(".next");
    // TODO(alexkirsz) This should accept a URL for assetPrefix.
    // let client_public_fs = VirtualFileSystemVc::new();
    // let client_public_root = client_public_fs.root();
    let workspace_fs = workspace_fs(&workspace_root, issue_reporter);
    let project_relative = project_root.strip_prefix(&workspace_root).unwrap();
    let project_relative = project_relative
        .strip_prefix(MAIN_SEPARATOR)
        .unwrap_or(project_relative)
        .replace(MAIN_SEPARATOR, "/");
    let project_root = workspace_fs.root().join(&project_relative);

    let node_root_ref = node_root.await?;

    let node_execution_chunking_context = DevChunkingContextVc::builder(
        project_root,
        node_root,
        node_root.join("chunks"),
        node_root.join("assets"),
        node_build_environment(),
    )
    .build()
    .into();

    let env = load_env(project_root);

    let execution_context =
        ExecutionContextVc::new(project_root, node_execution_chunking_context, env);
    let next_config = load_next_config(execution_context.with_layer("next_config"));

    let mode = NextMode::Build;
    let client_compile_time_info = get_client_compile_time_info(mode, browserslist_query);
    let server_compile_time_info = get_server_compile_time_info(mode, env, ServerAddrVc::empty());

    // TODO(alexkirsz) Pages should build their own routes, outside of a FS.
    let next_router_fs = VirtualFileSystemVc::new().as_file_system();
    let next_router_root = next_router_fs.root();
    let page_entries = get_page_entries(
        next_router_root,
        project_root,
        execution_context,
        env,
        client_compile_time_info,
        server_compile_time_info,
        next_config,
    );

    let app_entries = get_app_entries(
        project_root,
        execution_context,
        env,
        client_compile_time_info,
        server_compile_time_info,
        next_config,
    );

    handle_issues(page_entries, issue_reporter).await?;
    handle_issues(app_entries, issue_reporter).await?;

    let page_entries = page_entries.await?;
    let app_entries = app_entries.await?;

    let app_rsc_entries: Vec<_> = app_entries
        .entries
        .iter()
        .copied()
        .map(|entry| async move { Ok(entry.await?.rsc_entry) })
        .try_join()
        .await?;

    let app_client_references_by_entry = ClientReferencesByEntryVc::new(AssetsVc::cell(
        app_rsc_entries
            .iter()
            .copied()
            .map(|entry| entry.into())
            .collect(),
    ))
    .await?;

    let app_client_references: HashSet<_> = app_client_references_by_entry
        .values()
        .flatten()
        .copied()
        .collect();

    // The same client reference can occur from two different server components.
    // Here, we're only interested in deduped client references.
    let app_client_reference_tys: HashSet<_> = app_client_references
        .iter()
        .map(|client_reference| client_reference.ty())
        .copied()
        .collect();

    let app_ssr_entries: Vec<_> = app_client_reference_tys
        .iter()
        .map(|client_reference_ty| async move {
            let ClientReferenceType::EcmascriptClientReference(entry) = client_reference_ty else {
                return Ok(None);
            };

            Ok(Some(entry.await?.ssr_module))
        })
        .try_join()
        .await?
        .into_iter()
        .flatten()
        .collect();

    let page_ssr_entries = page_entries
        .entries
        .iter()
        .copied()
        .map(|entry| async move { Ok(entry.await?.ssr_module) })
        .try_join()
        .await?;

    let app_node_entries: Vec<_> = app_ssr_entries
        .iter()
        .copied()
        .chain(app_rsc_entries.iter().copied())
        .collect();

    let all_node_entries: Vec<_> = page_ssr_entries
        .iter()
        .copied()
        .chain(app_node_entries.iter().copied())
        .collect();

    // TODO(alexkirsz) Handle dynamic entries and dynamic chunks.
    let _dynamic_entries = NextDynamicEntriesVc::from_entries(AssetsVc::cell(
        all_node_entries
            .iter()
            .copied()
            .map(|entry| entry.into())
            .collect(),
    ))
    .await?;

    // TODO(alexkirsz) At this point, we have access to the whole module graph via
    // the entries. This is where we should compute unique module ids and optimized
    // chunks.

    // CHUNKING

    let client_chunking_context = get_client_chunking_context(
        project_root,
        client_root,
        client_compile_time_info.environment(),
        mode,
    );

    let server_chunking_context = get_server_chunking_context(
        project_root,
        node_root,
        client_root,
        server_compile_time_info.environment(),
    );
    // TODO(alexkirsz) This should be the same chunking context. The layer should
    // be applied on the AssetContext level instead.
    let rsc_chunking_context = server_chunking_context.with_layer("rsc");
    let ssr_chunking_context = server_chunking_context.with_layer("ssr");
    let (Some(rsc_chunking_context), Some(ssr_chunking_context)) = (
        BuildChunkingContextVc::resolve_from(rsc_chunking_context).await?,
        BuildChunkingContextVc::resolve_from(ssr_chunking_context).await?,
    ) else {
        bail!("with_layer should not change the type of the chunking context");
    };

    let mut all_chunks = vec![];

    let mut build_manifest: BuildManifest = Default::default();
    let build_manifest_path = client_root.join("build-manifest.json");

    // This ensures that the _next prefix is properly stripped from all client paths
    // in manifests. It will be added back on the client through the chunk_base_path
    // mechanism.
    let client_relative_path = client_root.join("_next");
    let client_relative_path_ref = client_relative_path.await?;

    // PAGE CHUNKING

    let mut pages_manifest: PagesManifest = Default::default();
    let pages_manifest_path = node_root.join("server/pages-manifest.json");
    let pages_manifest_dir_path = pages_manifest_path.parent().await?;

    compute_page_entries_chunks(
        &page_entries,
        client_chunking_context,
        ssr_chunking_context,
        node_root,
        &pages_manifest_dir_path,
        &client_relative_path_ref,
        &mut pages_manifest,
        &mut build_manifest,
        &mut all_chunks,
    )
    .await?;

    // APP CHUNKING

    let mut app_build_manifest = AppBuildManifest::default();
    let app_build_manifest_path = client_root.join("app-build-manifest.json");

    let mut app_paths_manifest = AppPathsManifest::default();
    let app_paths_manifest_path = node_root.join("server/app-paths-manifest.json");
    let app_paths_manifest_dir_path = app_paths_manifest_path.parent().await?;

    // APP CLIENT REFERENCES CHUNKING

    let app_client_references_chunks = compute_app_client_references_chunks(
        &app_client_reference_tys,
        client_chunking_context,
        ssr_chunking_context,
        &mut all_chunks,
    )
    .await?;

    // APP RSC CHUNKING
    // TODO(alexkirsz) Do some of that in parallel with the above.

    compute_app_entries_chunks(
        &app_entries,
        &app_client_references_by_entry,
        &app_client_references_chunks,
        rsc_chunking_context,
        client_chunking_context,
        ssr_chunking_context.into(),
        node_root,
        &client_relative_path_ref,
        &app_paths_manifest_dir_path,
        &mut app_build_manifest,
        &mut build_manifest,
        &mut app_paths_manifest,
        &mut all_chunks,
    )
    .await?;

    let mut completions = vec![];

    if let Some(build_context) = &options.build_context {
        let BuildContext { build_id, rewrites } = build_context;

        let ssg_manifest_path = format!("static/{build_id}/_ssgManifest.js");

        let ssg_manifest_fs_path = node_root.join(&ssg_manifest_path);
        completions.push(
            ssg_manifest_fs_path.write(
                FileContent::Content(
                    "self.__SSG_MANIFEST=new Set;self.__SSG_MANIFEST_CB&&self.__SSG_MANIFEST_CB()"
                        .into(),
                )
                .cell(),
            ),
        );

        build_manifest.low_priority_files.push(ssg_manifest_path);

        let sorted_pages =
            get_sorted_routes(&pages_manifest.pages.keys().cloned().collect::<Vec<_>>())?;

        let app_dependencies: HashSet<&str> = pages_manifest
            .pages
            .get("/_app")
            .iter()
            .map(|s| s.as_str())
            .collect();
        let mut pages = HashMap::new();

        for page in &sorted_pages {
            if page == "_app" {
                continue;
            }

            let dependencies = pages_manifest
                .pages
                .get(page)
                .iter()
                .map(|dep| dep.as_str())
                .filter(|dep| !app_dependencies.contains(*dep))
                .collect::<Vec<_>>();

            if !dependencies.is_empty() {
                pages.insert(page.to_string(), dependencies);
            }
        }

        let client_manifest = ClientBuildManifest {
            rewrites,
            sorted_pages: &sorted_pages,
            pages,
        };

        let client_manifest_path = format!("static/{build_id}/_buildManifest.js");

        let client_manifest_fs_path = node_root.join(&client_manifest_path);
        completions.push(
            client_manifest_fs_path.write(
                FileContent::Content(
                    format!(
                        "self.__BUILD_MANIFEST={};self.__BUILD_MANIFEST_CB && \
                         self.__BUILD_MANIFEST_CB()",
                        StringifyJs(&client_manifest)
                    )
                    .into(),
                )
                .cell(),
            ),
        );

        build_manifest.low_priority_files.push(client_manifest_path);
    }

    completions.push(write_manifest(pages_manifest, pages_manifest_path)?);
    completions.push(write_manifest(app_build_manifest, app_build_manifest_path)?);
    completions.push(write_manifest(app_paths_manifest, app_paths_manifest_path)?);
    completions.push(write_manifest(build_manifest, build_manifest_path)?);

    // Placeholder manifests.

    // TODO(alexkirsz) Proper middleware manifest with all (edge?) routes in it,
    // experimental-edge pages?
    completions.push(write_manifest(
        MiddlewaresManifest::default(),
        node_root.join("server/middleware-manifest.json"),
    )?);
    completions.push(write_manifest(
        NextFontManifest::default(),
        node_root.join("server/next-font-manifest.json"),
    )?);
    completions.push(write_manifest(
        FontManifest::default(),
        node_root.join("server/font-manifest.json"),
    )?);
    completions.push(write_manifest(
        ServerReferenceManifest::default(),
        node_root.join("server/server-reference-manifest.json"),
    )?);
    completions.push(write_manifest(
        ReactLoadableManifest::default(),
        node_root.join("react-loadable-manifest.json"),
    )?);

    completions.push(
        emit_all_assets(
            all_chunks,
            &node_root_ref,
            client_relative_path,
            client_root,
        )
        .await?,
    );

    Ok(CompletionsVc::all(completions))
}

#[turbo_tasks::function]
async fn workspace_fs(
    workspace_root: &str,
    issue_reporter: IssueReporterVc,
) -> Result<FileSystemVc> {
    let disk_fs = DiskFileSystemVc::new("workspace".to_string(), workspace_root.to_string());
    handle_issues(disk_fs, issue_reporter).await?;
    Ok(disk_fs.into())
}

#[turbo_tasks::function]
async fn node_fs(node_root: &str, issue_reporter: IssueReporterVc) -> Result<FileSystemVc> {
    let disk_fs = DiskFileSystemVc::new("node".to_string(), node_root.to_string());
    handle_issues(disk_fs, issue_reporter).await?;
    Ok(disk_fs.into())
}

#[turbo_tasks::function]
async fn client_fs(client_root: &str, issue_reporter: IssueReporterVc) -> Result<FileSystemVc> {
    let disk_fs = DiskFileSystemVc::new("client".to_string(), client_root.to_string());
    handle_issues(disk_fs, issue_reporter).await?;
    Ok(disk_fs.into())
}

async fn handle_issues<T: Into<RawVc> + CollectiblesSource + Copy>(
    source: T,
    issue_reporter: IssueReporterVc,
) -> Result<()> {
    let issues = IssueVc::peek_issues_with_path(source)
        .await?
        .strongly_consistent()
        .await?;

    let has_fatal = issue_reporter.report_issues(
        TransientInstance::new(issues.clone()),
        TransientValue::new(source.into()),
    );

    if *has_fatal.await? {
        Err(anyhow!("Fatal issue(s) occurred"))
    } else {
        Ok(())
    }
}

/// Emits all assets transitively reachable from the given chunks, that are
/// inside the node root or the client root.
async fn emit_all_assets(
    chunks: Vec<OutputAssetVc>,
    node_root: &FileSystemPath,
    client_relative_path: FileSystemPathVc,
    client_output_path: FileSystemPathVc,
) -> Result<CompletionVc> {
    let all_assets = all_assets_from_entries(OutputAssetsVc::cell(chunks)).await?;
    Ok(CompletionsVc::all(
        all_assets
            .iter()
            .copied()
            .map(|asset| async move {
                if asset.ident().path().await?.is_inside(node_root) {
                    return Ok(emit(asset));
                } else if asset
                    .ident()
                    .path()
                    .await?
                    .is_inside(&*client_relative_path.await?)
                {
                    // Client assets are emitted to the client output path, which is prefixed with
                    // _next. We need to rebase them to remove that prefix.
                    return Ok(emit_rebase(asset, client_relative_path, client_output_path));
                }

                Ok(CompletionVc::immutable())
            })
            .try_join()
            .await?,
    ))
}

#[turbo_tasks::function]
fn emit(asset: AssetVc) -> CompletionVc {
    asset.content().write(asset.ident().path())
}

#[turbo_tasks::function]
fn emit_rebase(asset: AssetVc, from: FileSystemPathVc, to: FileSystemPathVc) -> CompletionVc {
    asset
        .content()
        .write(rebase(asset.ident().path(), from, to))
}

/// Walks the asset graph from multiple assets and collect all referenced
/// assets.
#[turbo_tasks::function]
async fn all_assets_from_entries(entries: OutputAssetsVc) -> Result<AssetsVc> {
    Ok(AssetsVc::cell(
        AdjacencyMap::new()
            .skip_duplicates()
            .visit(
                entries.await?.iter().copied().map(Into::into),
                get_referenced_assets,
            )
            .await
            .completed()?
            .into_inner()
            .into_reverse_topological()
            .collect(),
    ))
}

/// Computes the list of all chunk children of a given chunk.
async fn get_referenced_assets(asset: AssetVc) -> Result<impl Iterator<Item = AssetVc> + Send> {
    Ok(asset
        .references()
        .await?
        .iter()
        .map(|reference| async move {
            let primary_assets = reference.resolve_reference().primary_assets().await?;
            Ok(primary_assets.clone_value())
        })
        .try_join()
        .await?
        .into_iter()
        .flatten())
}

/// Writes a manifest to disk. This consumes the manifest to ensure we don't
/// write to it afterwards.
fn write_manifest<T>(manifest: T, manifest_path: FileSystemPathVc) -> Result<CompletionVc>
where
    T: Serialize,
{
    let manifest_contents = serde_json::to_string_pretty(&manifest)?;
    Ok(manifest_path.write(FileContent::Content(manifest_contents.into()).cell()))
}

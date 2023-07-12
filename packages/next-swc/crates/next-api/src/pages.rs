use anyhow::{bail, Result};
use indexmap::IndexMap;
use next_core::{
    create_page_loader_entry_module,
    pages_structure::{
        PagesDirectoryStructure, PagesDirectoryStructureVc, PagesStructure, PagesStructureItem,
        PagesStructureVc,
    },
};
use turbo_tasks::{primitives::StringVc, CompletionVc};
use turbopack_binding::{
    turbo::tasks_fs::FileSystemPathVc,
    turbopack::{
        core::{
            chunk::{ChunkableModule, ChunkingContext},
            file_source::FileSourceVc,
        },
        ecmascript::EcmascriptModuleAssetVc,
    },
};

use crate::{
    project::ProjectVc,
    route::{Endpoint, EndpointVc, Route, RoutesVc, WrittenEndpointVc},
};

#[turbo_tasks::function]
pub async fn get_pages_routes(
    project: ProjectVc,
    page_structure: PagesStructureVc,
) -> Result<RoutesVc> {
    let PagesStructure { api, pages, .. } = *page_structure.await?;
    let mut routes = IndexMap::new();
    async fn add_dir_to_routes(
        routes: &mut IndexMap<String, Route>,
        dir: PagesDirectoryStructureVc,
        make_route: impl Fn(StringVc, StringVc, FileSystemPathVc) -> Route,
    ) -> Result<()> {
        let mut queue = vec![dir];
        while let Some(dir) = queue.pop() {
            let PagesDirectoryStructure {
                ref items,
                ref children,
                next_router_path: _,
                project_path: _,
            } = *dir.await?;
            for &item in items.iter() {
                let PagesStructureItem {
                    next_router_path,
                    project_path,
                    original_path,
                } = *item.await?;
                let pathname = format!("/{}", next_router_path.await?.path);
                let pathname_vc = StringVc::cell(pathname.clone());
                let original_name = StringVc::cell(format!("/{}", original_path.await?.path));
                let route = make_route(pathname_vc, original_name, project_path);
                routes.insert(pathname, route);
            }
            for &child in children.iter() {
                queue.push(child);
            }
        }
        Ok(())
    }
    if let Some(api) = api {
        add_dir_to_routes(&mut routes, api, |pathname, original_name, path| {
            Route::PageApi {
                endpoint: ApiEndpointVc::new(project, pathname, original_name, path).into(),
            }
        })
        .await?;
    }
    if let Some(page) = pages {
        add_dir_to_routes(&mut routes, page, |pathname, original_name, path| {
            Route::Page {
                html_endpoint: PageHtmlEndpointVc::new(
                    project,
                    pathname.clone(),
                    original_name.clone(),
                    path,
                )
                .into(),
                data_endpoint: PageDataEndpointVc::new(project, pathname, original_name, path)
                    .into(),
            }
        })
        .await?;
    }
    Ok(RoutesVc::cell(routes))
}

#[turbo_tasks::value]
struct PageHtmlEndpoint {
    project: ProjectVc,
    pathname: StringVc,
    original_name: StringVc,
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl PageHtmlEndpointVc {
    #[turbo_tasks::function]
    fn new(
        project: ProjectVc,
        pathname: StringVc,
        original_name: StringVc,
        path: FileSystemPathVc,
    ) -> Self {
        PageHtmlEndpoint {
            project,
            pathname,
            original_name,
            path,
        }
        .cell()
    }
}

#[turbo_tasks::value_impl]
impl Endpoint for PageHtmlEndpoint {
    #[turbo_tasks::function]
    async fn write_to_disk(&self) -> Result<WrittenEndpointVc> {
        let client_module = create_page_loader_entry_module(
            self.project.pages_client_module_context(),
            FileSourceVc::new(self.path).into(),
            self.pathname,
        );

        let Some(client_module) = EcmascriptModuleAssetVc::resolve_from(client_module).await?
        else {
            bail!("expected an ECMAScript module asset");
        };

        let client_chunking_context = self.project.client_chunking_context();

        let client_entry_chunk = client_module.as_root_chunk(client_chunking_context.into());

        let client_chunks = client_chunking_context.evaluated_chunk_group(
            client_entry_chunk,
            self.project
                .pages_client_runtime_entries()
                .with_entry(client_module.into()),
        );

        // TODO(alexkirsz) Needs to update the build manifest.

        todo!()
    }

    #[turbo_tasks::function]
    fn changed(&self) -> CompletionVc {
        todo!()
    }
}

#[turbo_tasks::value]
struct PageDataEndpoint {
    project: ProjectVc,
    pathname: StringVc,
    original_name: StringVc,
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl PageDataEndpointVc {
    #[turbo_tasks::function]
    fn new(
        project: ProjectVc,
        pathname: StringVc,
        original_name: StringVc,
        path: FileSystemPathVc,
    ) -> Self {
        PageDataEndpoint {
            project,
            pathname,
            original_name,
            path,
        }
        .cell()
    }
}

#[turbo_tasks::value_impl]
impl Endpoint for PageDataEndpoint {
    #[turbo_tasks::function]
    fn write_to_disk(&self) -> WrittenEndpointVc {
        todo!()
    }

    #[turbo_tasks::function]
    fn changed(&self) -> CompletionVc {
        todo!()
    }
}

#[turbo_tasks::value]
struct ApiEndpoint {
    project: ProjectVc,
    pathname: StringVc,
    original_name: StringVc,
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl ApiEndpointVc {
    #[turbo_tasks::function]
    fn new(
        project: ProjectVc,
        pathname: StringVc,
        original_name: StringVc,
        path: FileSystemPathVc,
    ) -> Self {
        ApiEndpoint {
            project,
            pathname,
            original_name,
            path,
        }
        .cell()
    }
}

#[turbo_tasks::value_impl]
impl Endpoint for ApiEndpoint {
    #[turbo_tasks::function]
    fn write_to_disk(&self) -> WrittenEndpointVc {
        todo!()
    }

    #[turbo_tasks::function]
    fn changed(&self) -> CompletionVc {
        todo!()
    }
}

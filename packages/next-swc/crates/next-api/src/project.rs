use std::path::MAIN_SEPARATOR;

use anyhow::Result;
use indexmap::IndexMap;
use next_core::app_structure::{find_app_dir, get_entrypoints};
use serde::{Deserialize, Serialize};
use turbo_tasks::{primitives::StringsVc, NothingVc, TaskInput, TransientValue};
use turbopack_binding::{
    turbo::tasks_fs::{DiskFileSystemVc, FileSystem, FileSystemPathVc, FileSystemVc},
    turbopack::core::PROJECT_FILESYSTEM_NAME,
};

use crate::{app::app_entry_point_to_route, route::RoutesVc};

#[derive(Serialize, Deserialize, Clone, TaskInput)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOptions {
    /// A root path from which all files must be nested under. Trying to access
    /// a file outside this root will fail. Think of this as a chroot.
    pub root_path: String,

    /// A path inside the root_path which contains the app/pages directories.
    pub project_path: String,

    /// Whether to watch he filesystem for file changes.
    pub watch: bool,
}

#[derive(Serialize, Deserialize, Clone, TaskInput)]
#[serde(rename_all = "camelCase")]
pub struct RoutesOptions {
    /// File extensions to scan inside our project
    pub page_extensions: Vec<String>,
}

#[turbo_tasks::value]
pub struct Project {
    /// A root path from which all files must be nested under. Trying to access
    /// a file outside this root will fail. Think of this as a chroot.
    root_path: FileSystemPathVc,

    /// A path inside the root_path which contains the app/pages directories.
    project_path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl ProjectVc {
    #[turbo_tasks::function]
    pub async fn new(options: ProjectOptions) -> Result<Self> {
        let fs = project_fs(&options.root_path, options.watch);
        let root = fs.root();
        let project_relative = options
            .project_path
            .strip_prefix(&options.root_path)
            .unwrap();
        let project_relative = project_relative
            .strip_prefix(MAIN_SEPARATOR)
            .unwrap_or(project_relative)
            .replace(MAIN_SEPARATOR, "/");
        let project_path = root.join(&project_relative);
        Ok(Project {
            root_path: root.resolve().await?,
            project_path: project_path.resolve().await?,
        }
        .cell())
    }

    /// Scans the app/pages directories for entry points files (matching the
    /// provided page_extensions).
    #[turbo_tasks::function]
    pub async fn routes(self, options: RoutesOptions) -> Result<RoutesVc> {
        let RoutesOptions { page_extensions } = options;
        let page_extensions = StringsVc::cell(page_extensions);
        let this = self.await?;
        let mut result = IndexMap::new();
        if let Some(app_dir) = *find_app_dir(this.project_path).await? {
            let app_entrypoints = get_entrypoints(app_dir, page_extensions);
            for (pathname, app_entrypoint) in app_entrypoints.await?.iter() {
                result.insert(pathname.clone(), app_entry_point_to_route(*app_entrypoint));
            }
        }
        Ok(RoutesVc::cell(result))
    }

    /// Emits opaque HMR events whenever a change is detected in the chunk group
    /// internally known as `identifier`.
    #[turbo_tasks::function]
    pub fn hmr_events(self, _identifier: String, _sender: TransientValue<()>) -> NothingVc {
        NothingVc::new()
    }
}

#[turbo_tasks::function]
async fn project_fs(project_dir: &str, watching: bool) -> Result<FileSystemVc> {
    let disk_fs =
        DiskFileSystemVc::new(PROJECT_FILESYSTEM_NAME.to_string(), project_dir.to_string());
    if watching {
        disk_fs.await?.start_watching_with_invalidation_reason()?;
    }
    Ok(disk_fs.into())
}

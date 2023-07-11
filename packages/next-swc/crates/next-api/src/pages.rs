use anyhow::Result;
use indexmap::IndexMap;
use next_core::pages_structure::{
    PagesDirectoryStructure, PagesDirectoryStructureVc, PagesStructure, PagesStructureItem,
    PagesStructureVc,
};
use turbo_tasks::CompletionVc;
use turbopack_binding::turbo::tasks_fs::FileSystemPathVc;

use crate::route::{Endpoint, EndpointVc, Route, RoutesVc, WrittenEndpointVc};

#[turbo_tasks::function]
pub async fn get_pages_routes(page_structure: PagesStructureVc) -> Result<RoutesVc> {
    let PagesStructure { api, pages, .. } = *page_structure.await?;
    let mut routes = IndexMap::new();
    async fn add_dir_to_routes(
        routes: &mut IndexMap<String, Route>,
        dir: PagesDirectoryStructureVc,
        make_route: impl Fn(FileSystemPathVc) -> Route,
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
                    original_path: _,
                } = *item.await?;
                let pathname = format!("/{}", next_router_path.await?.path);
                routes.insert(pathname, make_route(project_path));
            }
            for &child in children.iter() {
                queue.push(child);
            }
        }
        Ok(())
    }
    if let Some(api) = api {
        add_dir_to_routes(&mut routes, api, |path| Route::PageApi {
            endpoint: ApiEndpointVc::new(path).into(),
        })
        .await?;
    }
    if let Some(page) = pages {
        add_dir_to_routes(&mut routes, page, |path| Route::Page {
            html_endpoint: PageEndpointVc::new(path).into(),
            data_endpoint: PageDataEndpointVc::new(path).into(),
        })
        .await?;
    }
    Ok(RoutesVc::cell(routes))
}

#[turbo_tasks::value]
struct PageEndpoint {
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl PageEndpointVc {
    #[turbo_tasks::function]
    fn new(path: FileSystemPathVc) -> Self {
        PageEndpoint { path }.cell()
    }
}

#[turbo_tasks::value_impl]
impl Endpoint for PageEndpoint {
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
struct PageDataEndpoint {
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl PageDataEndpointVc {
    #[turbo_tasks::function]
    fn new(path: FileSystemPathVc) -> Self {
        PageDataEndpoint { path }.cell()
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
    path: FileSystemPathVc,
}

#[turbo_tasks::value_impl]
impl ApiEndpointVc {
    #[turbo_tasks::function]
    fn new(path: FileSystemPathVc) -> Self {
        ApiEndpoint { path }.cell()
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

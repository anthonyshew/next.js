use anyhow::Result;
use indexmap::IndexMap;
use turbo_tasks::CompletionVc;

#[turbo_tasks::value(shared)]
pub enum Route {
    Page {
        html_endpoint: EndpointVc,
        data_endpoint: EndpointVc,
    },
    PageApi {
        endpoint: EndpointVc,
    },
    AppPage {
        html_endpoint: EndpointVc,
        rsc_endpoint: EndpointVc,
    },
    AppRoute {
        endpoint: EndpointVc,
    },
    Conflict {
        routes: Vec<RouteVc>,
    },
}

#[turbo_tasks::value_trait]
pub trait Endpoint {
    fn write_to_disk(&self) -> WrittenEndpointVc;
    fn changed(&self) -> CompletionVc;
}

#[turbo_tasks::value]
#[derive(Debug)]
pub struct WrittenEndpoint {
    /// Relative to the root_path
    server_entry_path: String,
    /// Relative to the root_path
    server_paths: Vec<String>,
    /// Relative to the root_path
    client_paths: Vec<String>,
}

/// The routes as map from pathname to route. (pathname includes the leading
/// slash)
#[turbo_tasks::value(transparent)]
pub struct Routes(IndexMap<String, RouteVc>);

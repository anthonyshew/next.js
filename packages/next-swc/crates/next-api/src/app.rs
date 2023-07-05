use next_core::app_structure::Entrypoint;
use serde::{Deserialize, Serialize};
use turbo_tasks::trace::TraceRawVcs;

use crate::route::{Endpoint, EndpointVc, Route, RouteVc, WrittenEndpointVc};

#[turbo_tasks::function]
pub async fn app_entry_point_to_route(_entrypoint: Entrypoint) -> RouteVc {
    Route::Conflict { routes: vec![] }.cell()
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Debug, TraceRawVcs)]
enum AppPageEndpointType {
    Html,
    Rsc,
}

#[turbo_tasks::value]
struct AppPageEndpoint {
    ty: AppPageEndpointType,
}

#[turbo_tasks::value_impl]
impl Endpoint for AppPageEndpoint {
    #[turbo_tasks::function]
    fn write_to_disk(&self) -> WrittenEndpointVc {
        todo!()
    }
}

#[turbo_tasks::value]
struct AppRouteEndpoint;

#[turbo_tasks::value_impl]
impl Endpoint for AppRouteEndpoint {
    #[turbo_tasks::function]
    fn write_to_disk(&self) -> WrittenEndpointVc {
        todo!()
    }
}

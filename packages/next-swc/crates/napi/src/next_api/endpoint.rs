use napi::{bindgen_prelude::External, JsFunction};
use next_api::route::{Endpoint, EndpointVc, WrittenEndpoint};

use super::utils::{subscribe, RootTask, VcArc};

#[napi(object)]
pub struct NapiWrittenEndpoint {
    pub server_entry_path: String,
    pub server_paths: Vec<String>,
    pub client_paths: Vec<String>,
}

impl From<&WrittenEndpoint> for NapiWrittenEndpoint {
    fn from(written_endpoint: &WrittenEndpoint) -> Self {
        Self {
            server_entry_path: written_endpoint.server_entry_path.clone(),
            server_paths: written_endpoint.server_paths.clone(),
            client_paths: written_endpoint.client_paths.clone(),
        }
    }
}

#[napi]
pub async fn endpoint_write_to_disk(
    #[napi(ts_arg_type = "{ __napiType: \"Endpoint\" }")] endpoint: External<VcArc<EndpointVc>>,
) -> napi::Result<NapiWrittenEndpoint> {
    let turbo_tasks = endpoint.turbo_tasks().clone();
    let endpoint = **endpoint;
    let written = turbo_tasks
        .run_once(async move { Ok(endpoint.write_to_disk().strongly_consistent().await?) })
        .await?;
    Ok((&*written).into())
}

#[napi(ts_return_type = "{ __napiType: \"RootTask\" }")]
pub fn endpoint_changed_subscribe(
    #[napi(ts_arg_type = "{ __napiType: \"Endpoint\" }")] endpoint: External<VcArc<EndpointVc>>,
    func: JsFunction,
) -> napi::Result<External<RootTask>> {
    let turbo_tasks = endpoint.turbo_tasks().clone();
    let endpoint = **endpoint;
    subscribe(
        turbo_tasks,
        func,
        move || {
            let endpoint = endpoint.clone();
            async move {
                endpoint.changed().await?;
                Ok(())
            }
        },
        |_ctx| Ok(vec![()]),
    )
}

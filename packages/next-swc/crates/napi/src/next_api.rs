use std::{future::Future, sync::Arc};

use anyhow::{anyhow, Result};
use napi::{
    bindgen_prelude::{External, ToNapiValue},
    threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode},
    CallContext, JsFunction, Status,
};
use next_api::{
    project::{ProjectOptions, ProjectVc, RoutesOptions},
    route::{Endpoint, EndpointVc, RoutesReadRef},
};
use turbo_tasks::{NothingVc, TaskId, TurboTasks};
use turbopack_binding::{
    turbo::tasks_memory::MemoryBackend, turbopack::core::error::PrettyPrintError,
};

use crate::register;

pub struct VcArc<T> {
    turbo_tasks: Arc<TurboTasks<MemoryBackend>>,
    /// The Vc. Must be resolved, otherwise you are referencing an inactive
    /// operation.
    vc: T,
}

pub struct RootTask {
    turbo_tasks: Arc<TurboTasks<MemoryBackend>>,
    task_id: TaskId,
}

impl Drop for RootTask {
    fn drop(&mut self) {
        // TODO stop the root task
    }
}

#[napi(object)]
pub struct NapiProjectOptions {
    pub root_path: String,
    pub project_path: String,
    pub watch: bool,
    pub memory_limit: Option<f64>,
}

impl Into<ProjectOptions> for NapiProjectOptions {
    fn into(self) -> ProjectOptions {
        ProjectOptions {
            root_path: self.root_path,
            project_path: self.project_path,
            watch: self.watch,
        }
    }
}

#[napi(object)]
pub struct NapiRoutesOptions {
    pub page_extensions: Vec<String>,
}

impl Into<RoutesOptions> for NapiRoutesOptions {
    fn into(self) -> RoutesOptions {
        RoutesOptions {
            page_extensions: self.page_extensions,
        }
    }
}

#[napi(ts_return_type = "{ __napiType: \"Project\" }")]
pub async fn project_new(options: NapiProjectOptions) -> napi::Result<External<VcArc<ProjectVc>>> {
    register();
    let turbo_tasks = TurboTasks::new(MemoryBackend::new(
        options
            .memory_limit
            .map(|m| m as usize)
            .unwrap_or(usize::MAX),
    ));
    let options = options.into();
    let project = turbo_tasks
        .run_once(async move { Ok(ProjectVc::new(options).resolve().await?) })
        .await?;
    Ok(External::new_with_size_hint(
        VcArc {
            turbo_tasks,
            vc: project,
        },
        100,
    ))
}

#[napi(ts_return_type = "{ __napiType: \"RootTask\" }")]
pub fn project_routes_subscribe(
    #[napi(ts_arg_type = "{ __napiType: \"Project\" }")] project: External<VcArc<ProjectVc>>,
    options: NapiRoutesOptions,
    func: JsFunction,
) -> napi::Result<External<RootTask>> {
    let turbo_tasks = project.turbo_tasks.clone();
    let project = project.vc;
    let options: RoutesOptions = options.into();
    subscribe(
        turbo_tasks,
        func,
        move || {
            let options = options.clone();
            async move {
                let routes = project.routes(options).strongly_consistent().await?;
                Ok(routes)
            }
        },
        |ctx| {
            let routes = ctx.value;

            Ok(vec![format!("{:?}", routes)])
        },
    )
}

#[napi(object)]
struct NapiWrittenEndpoint {
    pub server_entry_path: String,
    pub server_paths: Vec<String>,
    pub client_paths: Vec<String>,
}

impl From<WrittenEndpoint> for NapiWrittenEndpoint {
    fn from(written_endpoint: WrittenEndpoint) -> Self {
        Self {
            server_entry_path: written_endpoint.server_entry_path,
            server_paths: written_endpoint.server_paths,
            client_paths: written_endpoint.client_paths,
        }
    }
}

#[napi]
pub async fn endpoint_write_to_disk(
    #[napi(ts_arg_type = "{ __napiType: \"Endpoint\" }")] endpoint: External<VcArc<EndpointVc>>,
) -> napi::Result<NapiWrittenEndpoint> {
    let turbo_tasks = endpoint.turbo_tasks.clone();
    let endpoint = endpoint.vc;
    let written = turbo_tasks
        .run_once(async move { Ok(endpoint.write_to_disk().strongly_consistent().await?) })
        .await?;
    Ok(written.into())
}

#[napi(ts_return_type = "{ __napiType: \"RootTask\" }")]
pub async fn endpoint_changed_subscribe(
    #[napi(ts_arg_type = "{ __napiType: \"Endpoint\" }")] endpoint: External<VcArc<EndpointVc>>,
    func: JsFunction,
) -> napi::Result<External<RootTask>> {
    let turbo_tasks = endpoint.turbo_tasks.clone();
    let endpoint = endpoint.vc;
    subscribe(
        turbo_tasks,
        move || {
            let endpoint = endpoint.clone();
            async move {
                let changed = endpoint.changed().await?;
                Ok(changed)
            }
        },
        |ctx| {
            let changed = ctx.value;
            Ok(vec![])
        },
    )
}

fn subscribe<T: 'static + Send + Sync, F: Future<Output = Result<T>> + Send, V: ToNapiValue>(
    turbo_tasks: Arc<TurboTasks<MemoryBackend>>,
    func: JsFunction,
    handler: impl 'static + Sync + Send + Clone + Fn() -> F,
    mapper: impl 'static + Sync + Send + FnMut(ThreadSafeCallContext<T>) -> napi::Result<Vec<V>>,
) -> napi::Result<External<RootTask>> {
    let func: ThreadsafeFunction<T> = func.create_threadsafe_function(0, mapper)?;
    let task_id = turbo_tasks.spawn_root_task(move || {
        let handler = handler.clone();
        let func = func.clone();
        Box::pin(async move {
            let result = handler().await;

            let status = func.call(
                result.map_err(|e| napi::Error::from_reason(PrettyPrintError(&e).to_string())),
                ThreadsafeFunctionCallMode::NonBlocking,
            );
            if !matches!(status, Status::Ok) {
                let error = anyhow!("Error calling JS function: {}", status);
                eprintln!("{}", error);
                return Err(error);
            }
            Ok(NothingVc::new().into())
        })
    });
    Ok(External::new(RootTask {
        turbo_tasks,
        task_id,
    }))
}

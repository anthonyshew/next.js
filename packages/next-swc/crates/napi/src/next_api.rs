use std::sync::Arc;

use napi::{bindgen_prelude::External, CallContext};
use turbo_tasks::TurboTasks;
use turbopack_binding::turbo::tasks_memory::MemoryBackend;

struct VcArc<T> {
    turbo_tasks: Arc<TurboTasks<MemoryBackend>>,
    vc: T,
}

#[napi(ts_return_type = "{ __napiType: \"Project\" }")]
fn project_new(options: ProjectOptions) -> napi::Result<External<VcArc<ProjectVc>>> {
    register();
    let turbo_tasks = Arc::new(TurboTasks::new(MemoryBackend::new(
        options
            .memory_limit
            .map(|m| m as usize)
            .unwrap_or(usize::MAX),
    )));
    let project = turbo_tasks
        .run_once(async move { Ok(ProjectVc::new(options)) })
        .await?;
    Ok(External::new_with_size_hint(
        VcArc {
            turbo_tasks,
            vc: project,
        },
        100,
    ))
}

#[napi(ts_return_type = "{ __napiType: \"Entrypoints\" }")]
fn project_entrypoints(
    #[napi(ts_arg_type = "{ __napiType: \"Project\" }")] project: External<VcArc<ProjectVc>>,
) -> napi::Result<External<VcArc<EntrypointsVc>>> {
    let turbo_tasks = project.turbo_tasks.clone();
    let entrypoints = turbo_tasks
        .run_once(async move {
            let project = project.vc;
            let entrypoints = project.entry_points();
            Ok(entrypoints)
        })
        .await?;
    Ok(VcArc {
        turbo_tasks: project.turbo_tasks,
        vc: entrypoints,
    })
}

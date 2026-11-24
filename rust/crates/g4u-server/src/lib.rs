//! g4u-server — IPC 命令总线（Tauri command 薄壳）。
//! 只做参数转发与阻塞线程调度；所有 Git 语义在 g4u-ops。
//!
//! Tauri 的 `#[tauri::command]` 不能标在 crate 根模块的 pub 函数上（宏卫生限制），
//! 因此命令统一放在 `commands` 子模块。

use std::sync::Arc;

use g4u_model::G4uError;
use g4u_ops::RepoService;

pub mod commands;

pub struct ServerState {
    pub service: Arc<RepoService>,
}

pub(crate) async fn blocking<T, F>(work: F) -> Result<T, G4uError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, G4uError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result,
        Err(err) => Err(G4uError::internal(format!("后台任务失败: {err}"))),
    }
}

//! Tauri command 定义（必须位于子模块，见 crate 文档）。

use g4u_model::{
    AppInfo, BackupRef, BranchListing, CommitDetail, CommitSummary, CredentialEntry,
    CredentialInput, CredentialValidation, EnvReport, EolReport, FileDiff, FilePreview, G4uError,
    GitignoreFile, IgnoreMatch, LargeFile, LockReport, ObjectStats, Oid, ProxyEntry, ProxyInput,
    PushAuditReport, ReflogEntry, RemoteInfo, RepoSettings, RepoStatus, RepoSummary, SecretFinding,
    SettingsScope, SettingsView,
};
use tauri::{Manager, State};

use crate::{blocking, ServerState};

#[tauri::command]
pub async fn app_info(
    state: State<'_, ServerState>,
    app: tauri::AppHandle,
) -> Result<AppInfo, G4uError> {
    let version = app.package_info().version.to_string();
    let service = state.service.clone();
    blocking(move || Ok(service.app_info(version))).await
}

#[tauri::command]
pub async fn repo_list(state: State<'_, ServerState>) -> Result<Vec<RepoSummary>, G4uError> {
    let service = state.service.clone();
    blocking(move || Ok(service.list())).await
}

#[tauri::command]
pub async fn repo_open(
    state: State<'_, ServerState>,
    path: String,
) -> Result<RepoSummary, G4uError> {
    let service = state.service.clone();
    blocking(move || service.open(&path)).await
}

#[tauri::command]
pub async fn repo_remove(state: State<'_, ServerState>, repo_id: String) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.remove(&repo_id)).await
}

#[tauri::command]
pub async fn repo_status(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<RepoStatus, G4uError> {
    let service = state.service.clone();
    blocking(move || service.status(&repo_id)).await
}

#[tauri::command]
pub async fn repo_log(
    state: State<'_, ServerState>,
    repo_id: String,
    limit: Option<u32>,
) -> Result<Vec<CommitSummary>, G4uError> {
    let service = state.service.clone();
    let limit = limit.unwrap_or(500) as usize;
    blocking(move || service.log(&repo_id, limit)).await
}

#[tauri::command]
pub async fn repo_branches(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<BranchListing, G4uError> {
    let service = state.service.clone();
    blocking(move || service.branches(&repo_id)).await
}

#[tauri::command]
pub async fn repo_diff(
    state: State<'_, ServerState>,
    repo_id: String,
    path: String,
    staged: bool,
) -> Result<FileDiff, G4uError> {
    let service = state.service.clone();
    blocking(move || service.diff(&repo_id, &path, staged)).await
}

#[tauri::command]
pub async fn repo_gitignore(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<GitignoreFile, G4uError> {
    let service = state.service.clone();
    blocking(move || service.gitignore(&repo_id)).await
}

#[tauri::command]
pub async fn repo_write_gitignore(
    state: State<'_, ServerState>,
    repo_id: String,
    content: String,
    append: bool,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.write_gitignore(&repo_id, &content, append)).await
}

#[tauri::command]
pub async fn repo_files(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<Vec<String>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.files(&repo_id)).await
}

#[tauri::command]
pub async fn repo_file_preview(
    state: State<'_, ServerState>,
    repo_id: String,
    path: String,
) -> Result<FilePreview, G4uError> {
    let service = state.service.clone();
    blocking(move || service.file_preview(&repo_id, &path)).await
}

#[tauri::command]
pub async fn repo_large_files(
    state: State<'_, ServerState>,
    repo_id: String,
    limit: usize,
) -> Result<Vec<LargeFile>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.large_files(&repo_id, limit)).await
}

#[tauri::command]
pub async fn repo_backup_refs(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<Vec<BackupRef>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.backup_refs(&repo_id)).await
}

#[tauri::command]
pub async fn repo_check_ignore(
    state: State<'_, ServerState>,
    repo_id: String,
    path: String,
) -> Result<Option<IgnoreMatch>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.ignore_check(&repo_id, &path)).await
}

#[tauri::command]
pub async fn repo_eol_report(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<EolReport, G4uError> {
    let service = state.service.clone();
    blocking(move || service.eol_report(&repo_id)).await
}

#[tauri::command]
pub async fn repo_object_stats(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<ObjectStats, G4uError> {
    let service = state.service.clone();
    blocking(move || service.object_stats(&repo_id)).await
}

#[tauri::command]
pub async fn repo_fsck(state: State<'_, ServerState>, repo_id: String) -> Result<String, G4uError> {
    let service = state.service.clone();
    blocking(move || service.fsck(&repo_id)).await
}

#[tauri::command]
pub async fn repo_reflog(
    state: State<'_, ServerState>,
    repo_id: String,
    limit: usize,
) -> Result<Vec<ReflogEntry>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.reflog(&repo_id, limit)).await
}

#[tauri::command]
pub async fn repo_lock_report(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<LockReport, G4uError> {
    let service = state.service.clone();
    blocking(move || service.lock_report(&repo_id)).await
}

#[tauri::command]
pub async fn repo_env_report(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<EnvReport, G4uError> {
    let service = state.service.clone();
    blocking(move || service.env_report(&repo_id)).await
}

#[tauri::command]
pub async fn repo_secret_scan(
    state: State<'_, ServerState>,
    repo_id: String,
    limit: usize,
) -> Result<Vec<SecretFinding>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.secret_scan(&repo_id, limit)).await
}

#[tauri::command]
pub async fn repo_commit_detail(
    state: State<'_, ServerState>,
    repo_id: String,
    oid: String,
) -> Result<CommitDetail, G4uError> {
    let service = state.service.clone();
    blocking(move || service.commit_detail(&repo_id, &oid)).await
}

#[tauri::command]
pub async fn index_stage(
    state: State<'_, ServerState>,
    repo_id: String,
    paths: Vec<String>,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.stage(&repo_id, &paths)).await
}

#[tauri::command]
pub async fn index_unstage(
    state: State<'_, ServerState>,
    repo_id: String,
    paths: Vec<String>,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.unstage(&repo_id, &paths)).await
}

#[tauri::command]
pub async fn commit_create(
    state: State<'_, ServerState>,
    repo_id: String,
    message: String,
    amend: bool,
    run_hooks: bool,
) -> Result<Oid, G4uError> {
    let service = state.service.clone();
    blocking(move || service.commit(&repo_id, &message, amend, run_hooks)).await
}

#[tauri::command]
pub async fn repo_undo_last(
    state: State<'_, ServerState>,
    repo_id: String,
    expected_after: String,
) -> Result<Option<Oid>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.undo_last(&repo_id, &expected_after)).await
}

#[tauri::command]
pub async fn branch_checkout(
    state: State<'_, ServerState>,
    repo_id: String,
    name: String,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.checkout_branch(&repo_id, &name)).await
}

#[tauri::command]
pub async fn branch_create(
    state: State<'_, ServerState>,
    repo_id: String,
    name: String,
    start: Option<String>,
    checkout: bool,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.create_branch(&repo_id, &name, start.as_deref(), checkout)).await
}

#[tauri::command]
pub async fn repo_settings(
    state: State<'_, ServerState>,
    repo_id: Option<String>,
) -> Result<SettingsView, G4uError> {
    let service = state.service.clone();
    blocking(move || Ok(service.settings(repo_id.as_deref()))).await
}

#[tauri::command]
pub async fn repo_save_settings(
    state: State<'_, ServerState>,
    scope: SettingsScope,
    repo_id: Option<String>,
    settings: RepoSettings,
) -> Result<SettingsView, G4uError> {
    let service = state.service.clone();
    blocking(move || service.save_settings(scope, repo_id.as_deref(), settings)).await
}

#[tauri::command]
pub async fn repo_remotes(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<Vec<RemoteInfo>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.remotes(&repo_id)).await
}

#[tauri::command]
pub async fn repo_fetch(
    state: State<'_, ServerState>,
    repo_id: String,
    remote: Option<String>,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.fetch(&repo_id, remote.as_deref())).await
}

#[tauri::command]
pub async fn repo_pull(state: State<'_, ServerState>, repo_id: String) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.pull(&repo_id)).await
}

#[tauri::command]
pub async fn repo_push_audit(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<PushAuditReport, G4uError> {
    let service = state.service.clone();
    blocking(move || service.push_audit(&repo_id)).await
}

#[tauri::command]
pub async fn repo_push(
    state: State<'_, ServerState>,
    repo_id: String,
    remote: Option<String>,
    set_upstream: bool,
    allow_risky: bool,
) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.push(&repo_id, remote.as_deref(), set_upstream, allow_risky)).await
}

#[tauri::command]
pub async fn credential_list(
    state: State<'_, ServerState>,
) -> Result<Vec<CredentialEntry>, G4uError> {
    let service = state.service.clone();
    blocking(move || Ok(service.credentials())).await
}

#[tauri::command]
pub async fn credential_save(
    state: State<'_, ServerState>,
    input: CredentialInput,
) -> Result<CredentialEntry, G4uError> {
    let service = state.service.clone();
    blocking(move || service.save_credential(input)).await
}

#[tauri::command]
pub async fn credential_delete(state: State<'_, ServerState>, id: String) -> Result<(), G4uError> {
    let service = state.service.clone();
    blocking(move || service.delete_credential(&id)).await
}

#[tauri::command]
pub async fn credential_validate(
    state: State<'_, ServerState>,
    repo_id: String,
    id: String,
) -> Result<CredentialValidation, G4uError> {
    let service = state.service.clone();
    blocking(move || service.validate_credential(&repo_id, &id)).await
}

#[tauri::command]
pub async fn settings_save_proxy(
    state: State<'_, ServerState>,
    input: ProxyInput,
) -> Result<Vec<ProxyEntry>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.save_proxy(input)).await
}

#[tauri::command]
pub async fn settings_reset(state: State<'_, ServerState>) -> Result<SettingsView, G4uError> {
    let service = state.service.clone();
    blocking(move || service.reset_settings()).await
}

#[tauri::command]
pub async fn settings_delete_proxy(
    state: State<'_, ServerState>,
    id: String,
) -> Result<Vec<ProxyEntry>, G4uError> {
    let service = state.service.clone();
    blocking(move || service.delete_proxy(&id)).await
}

#[tauri::command]
pub async fn repo_cleanup_backups(
    state: State<'_, ServerState>,
    repo_id: String,
) -> Result<usize, G4uError> {
    let service = state.service.clone();
    blocking(move || service.cleanup_backups(&repo_id)).await
}

fn push_query_param(params: &mut Vec<String>, key: &str, value: Option<&str>) {
    let Some(value) = value else { return };
    if value.is_empty()
        || value.len() > 64
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return;
    }
    params.push(format!("{key}={value}"));
}

#[tauri::command]
pub async fn open_settings_window(
    app: tauri::AppHandle,
    repo_id: Option<String>,
    title: Option<String>,
    section: Option<String>,
    credential: Option<String>,
) -> Result<(), String> {
    let mut params = Vec::new();
    push_query_param(&mut params, "repo", repo_id.as_deref());
    push_query_param(&mut params, "section", section.as_deref());
    push_query_param(&mut params, "credential", credential.as_deref());
    // 每次打开都换一个 hash，已打开的设置窗口才能重新应用深链（例如再次更新同一凭证）。
    let focus = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    params.push(format!("focus={focus}"));
    let query = format!("?{}", params.join("&"));

    if let Some(existing) = app.get_webview_window("settings") {
        if let Some(title) = title.as_deref() {
            let _ = existing.set_title(title);
        }
        if !query.is_empty() {
            let _ = existing.eval(format!(
                "window.location.hash = '#/settings-window{query}';"
            ));
        }
        let _ = existing.set_focus();
        return Ok(());
    }

    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App(format!("index.html#/settings-window{query}").into()),
    )
    .title(title.unwrap_or_else(|| "Git4U 设置".into()))
    .inner_size(1000.0, 720.0)
    .min_inner_size(900.0, 600.0)
    .center()
    .build()
    .map_err(|err| err.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn close_settings_window(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|err| err.to_string())
}

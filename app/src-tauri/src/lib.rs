use std::sync::Arc;

use g4u_ops::RepoService;
use g4u_server::ServerState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(ServerState {
                service: Arc::new(RepoService::new(dir)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            g4u_server::commands::app_info,
            g4u_server::commands::repo_list,
            g4u_server::commands::repo_open,
            g4u_server::commands::repo_remove,
            g4u_server::commands::repo_status,
            g4u_server::commands::repo_log,
            g4u_server::commands::repo_branches,
            g4u_server::commands::repo_diff,
            g4u_server::commands::repo_gitignore,
            g4u_server::commands::repo_write_gitignore,
            g4u_server::commands::repo_files,
            g4u_server::commands::repo_file_preview,
            g4u_server::commands::repo_large_files,
            g4u_server::commands::repo_backup_refs,
            g4u_server::commands::repo_check_ignore,
            g4u_server::commands::repo_eol_report,
            g4u_server::commands::repo_object_stats,
            g4u_server::commands::repo_fsck,
            g4u_server::commands::repo_reflog,
            g4u_server::commands::repo_lock_report,
            g4u_server::commands::repo_env_report,
            g4u_server::commands::repo_secret_scan,
            g4u_server::commands::repo_commit_detail,
            g4u_server::commands::index_stage,
            g4u_server::commands::index_unstage,
            g4u_server::commands::commit_create,
            g4u_server::commands::repo_undo_last,
            g4u_server::commands::branch_checkout,
            g4u_server::commands::branch_create,
            g4u_server::commands::repo_settings,
            g4u_server::commands::repo_save_settings,
            g4u_server::commands::repo_remotes,
            g4u_server::commands::repo_fetch,
            g4u_server::commands::repo_pull,
            g4u_server::commands::repo_push_audit,
            g4u_server::commands::repo_push,
            g4u_server::commands::credential_list,
            g4u_server::commands::credential_save,
            g4u_server::commands::credential_delete,
            g4u_server::commands::credential_validate,
            g4u_server::commands::settings_save_proxy,
            g4u_server::commands::settings_delete_proxy,
            g4u_server::commands::settings_reset,
            g4u_server::commands::repo_cleanup_backups,
            g4u_server::commands::open_settings_window,
            g4u_server::commands::close_settings_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Git4U");
}

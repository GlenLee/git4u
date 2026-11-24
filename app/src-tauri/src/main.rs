#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::var_os("G4U_ASKPASS").is_some() {
        askpass();
        return;
    }
    git4u_desktop_lib::run()
}

/// Git 通过 `GIT_ASKPASS` 调起本进程时，只输出本次凭据，不启动 Tauri。
fn askpass() {
    use std::io::Write;

    let prompt = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let value = if prompt.contains("username") {
        std::env::var("G4U_ASKPASS_USER")
    } else {
        std::env::var("G4U_ASKPASS_PASS")
    }
    .unwrap_or_default();
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{value}");
    let _ = out.flush();
}

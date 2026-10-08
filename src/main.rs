#![cfg_attr(windows, windows_subsystem = "windows")]
fn main() {
    #[cfg(windows)]
    if let Err(error) = nanfeng_codex_quota::desktop::run() {
        nanfeng_codex_quota::desktop::show_error(&format!("{error:#}"));
    }
}

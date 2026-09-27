// 防止 Windows release 构建附加控制台窗口；debug 构建保留控制台便于看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    altgo_tauri::run()
}

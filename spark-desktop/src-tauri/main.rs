// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use spark_core::messages;

fn main() {
  tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![send_message, get_messages])
    .run(tauri::generate_context!(0))
    .expect("error while running Tauri app");
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
fn main() {
    if let Err(error) = powerwatch_app_lib::run() {
        eprintln!("PowerWatch startup failed: {error}");
        std::process::exit(1);
    }
}

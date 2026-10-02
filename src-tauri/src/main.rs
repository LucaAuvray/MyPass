// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Check if launched as Native Messaging Host by the browser
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--native-messaging") {
        mypass_lib::run_native_messaging_proxy();
    } else {
        mypass_lib::run()
    }
}

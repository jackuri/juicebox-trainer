// The same app on a PC (for development); the phone build starts from lib.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    juicebox_remote_lib::run();
}

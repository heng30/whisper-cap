#![windows_subsystem = "windows"]

#[tokio::main]
async fn main() {
    extern crate whispercap;
    whispercap::desktop_main().await;
}

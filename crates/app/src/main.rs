//! Hikidashi: Windows向けの自分専用ランチャー. 画面とOSの部分(Win32). 中心部は`hikidashi-core`.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Hikidashi runs on Windows only. Build with --target x86_64-pc-windows-gnu.");
}

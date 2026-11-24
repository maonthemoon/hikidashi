//! 開いている窓とプロセスの一覧. 窓の切り替え(H4), 起動中のアプリを前に出す(C3), kill(G1)で共通に使う.

use super::util::from_wide;
use hikidashi_core::catalog::{Item, Kind};
use std::collections::HashMap;
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

/// 開いている窓
#[derive(Debug, Clone)]
pub struct Win {
    pub hwnd: isize,
    pub title: String,
    pub pid: u32,
    pub exe: String,
}

/// プロセスのexeのフルパス
pub fn process_path(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if ok {
            from_wide(&buf[..len as usize])
        } else {
            String::new()
        }
    }
}

struct EnumState {
    out: Vec<Win>,
    own: isize,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let st = &mut *(lparam.0 as *mut EnumState);
    if hwnd.0 as isize == st.own || !IsWindowVisible(hwnd).as_bool() {
        return true.into();
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
    if ex & WS_EX_TOOLWINDOW.0 != 0 || (!owner.is_invalid() && ex & WS_EX_APPWINDOW.0 == 0) {
        return true.into();
    }
    // 別の仮想デスクトップの窓や, 隠れたストアアプリの窓を除く
    let mut cloaked = 0u32;
    if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4).is_ok() && cloaked != 0 {
        return true.into();
    }
    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return true.into();
    }
    let mut buf = vec![0u16; len as usize + 1];
    GetWindowTextW(hwnd, &mut buf);
    let title = from_wide(&buf);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    st.out.push(Win { hwnd: hwnd.0 as isize, title, pid, exe: String::new() });
    true.into()
}

/// 開いている窓の一覧(Alt+Tabに出るもの). `own`は自分の窓(除く)
pub fn windows(own: isize) -> Vec<Win> {
    let mut st = EnumState { out: Vec::new(), own };
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut st as *mut _ as isize));
    }
    let mut paths: HashMap<u32, String> = HashMap::new();
    for w in st.out.iter_mut() {
        w.exe = paths.entry(w.pid).or_insert_with(|| process_path(w.pid)).clone();
    }
    st.out
}

fn exe_name(path: &str) -> String {
    path.rsplit('\\').next().unwrap_or(path).to_string()
}

/// 窓の切り替え(H4)の一覧. dataに窓のハンドル, argsにexeのパス(アイコン用)
pub fn window_items(wins: &[Win]) -> Vec<Item> {
    wins.iter()
        .map(|w| {
            let mut it = Item::new(Kind::Window, w.title.clone(), exe_name(&w.exe), w.hwnd.to_string());
            it.data = w.hwnd as u64;
            it.args = w.exe.clone();
            it
        })
        .collect()
}

/// killの一覧(G1). 窓のあるプロセスは窓のタイトルで1行ずつ, 窓のないものは名前でまとめる.
/// targetにプロセスIDをカンマ区切りで入れる.
pub fn process_items(wins: &[Win]) -> Vec<Item> {
    let mut out = Vec::new();
    let mut win_count: HashMap<u32, usize> = HashMap::new();
    for w in wins {
        *win_count.entry(w.pid).or_default() += 1;
    }
    for w in wins {
        let others = win_count.get(&w.pid).copied().unwrap_or(1) > 1;
        let sub = if others { format!("{} · Also closes its other windows", exe_name(&w.exe)) } else { exe_name(&w.exe) };
        let mut it = Item::new(Kind::Process, w.title.clone(), sub, w.pid.to_string());
        it.args = w.exe.clone();
        it.keys.push(exe_name(&w.exe));
        out.push(it);
    }
    // 窓のないプロセスを名前でまとめる
    let mut groups: HashMap<String, Vec<u32>> = HashMap::new();
    let own = std::process::id();
    unsafe {
        if let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            let mut pe = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            if Process32FirstW(snap, &mut pe).is_ok() {
                loop {
                    let pid = pe.th32ProcessID;
                    if pid > 4 && pid != own && !win_count.contains_key(&pid) {
                        let name = from_wide(&pe.szExeFile);
                        groups.entry(name).or_default().push(pid);
                    }
                    if Process32NextW(snap, &mut pe).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snap);
        }
    }
    // Windowsの中心のプロセスは出さない(終了するとWindowsが止まる)
    const SYSTEM: &[&str] = &[
        "system", "registry", "smss.exe", "csrss.exe", "wininit.exe", "winlogon.exe", "services.exe", "lsass.exe", "svchost.exe", "fontdrvhost.exe", "dwm.exe",
        "memory compression", "secure system", "lsaiso.exe", "sihost.exe", "ctfmon.exe", "explorer.exe", "conhost.exe", "msmpeng.exe", "audiodg.exe",
    ];
    let mut names: Vec<_> = groups.into_iter().filter(|(n, _)| !SYSTEM.contains(&n.to_lowercase().as_str())).collect();
    names.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    for (name, pids) in names {
        let shown = if pids.len() > 1 { format!("{} ×{}", name, pids.len()) } else { name.clone() };
        let ids: Vec<String> = pids.iter().map(|p| p.to_string()).collect();
        let mut it = Item::new(Kind::Process, shown, "No window", ids.join(","));
        it.keys.push(name);
        out.push(it);
    }
    out
}

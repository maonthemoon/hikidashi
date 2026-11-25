//! タスクトレイ(A6), 自動起動(A6), ホットキー(A1), IME(J4), メモリの量, フォント.

use super::util::{from_wide, wide, WStr};
use hikidashi_core::hotkey::Hotkey;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Gdi::{EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, TEXTMETRICW};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Registry::{RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Input::Ime::{ImmGetContext, ImmReleaseContext, ImmSetOpenStatus};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::{LoadIconW, HICON, IDI_APPLICATION};

pub const WM_APP_TRAY: u32 = 0x8000 + 1;
pub const HOTKEY_ID: i32 = 1;
/// 選んだ文字で開くホットキー
pub const HOTKEY_SELECTION_ID: i32 = 2;
const MOD_NOREPEAT: u32 = 0x4000;

pub fn app_icon() -> HICON {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap_or_default();
        // リソースの1番(hikidashi.rc)
        LoadIconW(Some(hinst.into()), PCWSTR(1 as *const u16)).or_else(|_| LoadIconW(None, IDI_APPLICATION)).unwrap_or_default()
    }
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
    d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    d.uCallbackMessage = WM_APP_TRAY;
    d.hIcon = app_icon();
    let tip = wide("Hikidashi");
    d.szTip[..tip.len()].copy_from_slice(&tip);
    d
}

pub fn tray_add(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &tray_data(hwnd));
    }
}

pub fn tray_remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &tray_data(hwnd));
    }
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_NAME: PCWSTR = w!("Hikidashi");

fn open_run(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), access, &mut key).ok().ok()? };
    Some(key)
}

/// 自動起動が登録されているか
pub fn autostart_enabled() -> bool {
    autostart_value().is_some()
}

/// 自動起動に登録している文字
fn autostart_value() -> Option<String> {
    let key = open_run(KEY_READ)?;
    let mut buf = [0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe { RegQueryValueExW(key, RUN_NAME, None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len)).is_ok() };
    unsafe {
        let _ = RegCloseKey(key);
    }
    Some(from_wide(&buf)).filter(|v| ok && !v.is_empty())
}

fn autostart_command(exe: &str) -> String {
    format!("\"{exe}\" --autostart")
}

/// 自動起動の登録先が別の場所なら, 今のexeに直す(登録していなければ何もしない)
pub fn fix_autostart(exe: &str) {
    if autostart_value().is_some_and(|v| !v.eq_ignore_ascii_case(&autostart_command(exe))) {
        set_autostart(true, exe);
    }
}

/// 自動起動を登録する(HKCUのRunキー. 管理者権限は不要)
pub fn set_autostart(on: bool, exe: &str) {
    let Some(key) = open_run(KEY_SET_VALUE) else { return };
    unsafe {
        if on {
            let v = wide(&autostart_command(exe));
            let bytes = std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len() * 2);
            let _ = RegSetValueExW(key, RUN_NAME, Some(0), REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(key, RUN_NAME);
        }
        let _ = RegCloseKey(key);
    }
}

/// HKCUの文字列の値を読む(既定のブラウザーなど)
pub fn reg_string(subkey: &str, name: &str) -> Option<String> {
    let k = WStr::new(subkey);
    let n = WStr::new(name);
    let mut key = HKEY::default();
    unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, k.pcwstr(), Some(0), KEY_READ, &mut key).ok().ok()? };
    let mut buf = [0u16; 512];
    let mut len = (buf.len() * 2) as u32;
    let ok = unsafe { RegQueryValueExW(key, n.pcwstr(), None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut len)).is_ok() };
    unsafe {
        let _ = RegCloseKey(key);
    }
    let s = from_wide(&buf);
    (ok && !s.is_empty()).then_some(s)
}

/// ホットキーを登録する. 結果はログに書く(会社PCで保存のあとホットキーが効かなくなった件の調べ)
pub fn register_hotkey(hwnd: HWND, h: Hotkey) -> bool {
    register_hotkey_id(hwnd, HOTKEY_ID, h)
}

pub fn register_hotkey_id(hwnd: HWND, id: i32, h: Hotkey) -> bool {
    let r = unsafe { RegisterHotKey(Some(hwnd), id, HOT_KEY_MODIFIERS(h.mods | MOD_NOREPEAT), h.vk) };
    match &r {
        Ok(()) => super::storage::log(&format!("hotkey {id} registered: {}", h.format())),
        Err(e) => super::storage::log(&format!("hotkey {id} failed: {} ({})", h.format(), e.message())),
    }
    r.is_ok()
}

pub fn unregister_hotkey(hwnd: HWND) {
    unregister_hotkey_id(hwnd, HOTKEY_ID);
}

pub fn unregister_hotkey_id(hwnd: HWND, id: i32) {
    super::storage::log(&format!("hotkey {id} unregistered"));
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), id);
    }
}

/// 窓を出したら日本語入力をオフ(英数)にする(J4)
pub fn ime_off(edit: HWND) {
    unsafe {
        let himc = ImmGetContext(edit);
        if !himc.is_invalid() {
            let _ = ImmSetOpenStatus(himc, false);
            let _ = ImmReleaseContext(edit, himc);
        }
    }
}

/// (ワーキングセット, プライベート)のメモリの量(バイト)
pub fn memory() -> (usize, usize) {
    unsafe {
        let mut c = PROCESS_MEMORY_COUNTERS_EX { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
        if GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb).is_ok() {
            (c.WorkingSetSize, c.PrivateUsage)
        } else {
            (0, 0)
        }
    }
}

unsafe extern "system" fn font_found(_lf: *const LOGFONTW, _tm: *const TEXTMETRICW, _ty: u32, lparam: LPARAM) -> i32 {
    *(lparam.0 as *mut bool) = true;
    0
}

/// そのフォントが入っているか
pub fn font_exists(name: &str) -> bool {
    let mut lf = LOGFONTW { lfCharSet: DEFAULT_CHARSET, ..Default::default() };
    let n = wide(name);
    if n.len() > lf.lfFaceName.len() {
        return false;
    }
    lf.lfFaceName[..n.len()].copy_from_slice(&n);
    let mut found = false;
    unsafe {
        let hdc = GetDC(None);
        EnumFontFamiliesExW(hdc, &lf, Some(font_found), LPARAM(&mut found as *mut _ as isize), 0);
        ReleaseDC(None, hdc);
    }
    found
}

/// 最初に見つかったフォント
pub fn pick_font(names: &[&str]) -> String {
    names.iter().find(|n| font_exists(n)).map(|n| n.to_string()).unwrap_or_else(|| names.last().unwrap().to_string())
}

/// 知らせるだけの窓. 画面のスレッドを止めないよう, 別のスレッドで出す
pub fn notice(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND};
    super::storage::log(&format!("notice: {text}"));
    let t = super::i18n::tr(text);
    std::thread::spawn(move || {
        let (t, c) = (WStr::new(&t), WStr::new("Hikidashi"));
        unsafe { MessageBoxW(None, t.pcwstr(), c.pcwstr(), MB_OK | MB_ICONWARNING | MB_SETFOREGROUND) };
    });
}

pub fn message_box_yes_no(hwnd: HWND, text: &str, title: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_YESNO};
    let t = WStr::new(&super::i18n::tr(text));
    let c = WStr::new(&super::i18n::tr(title));
    unsafe { MessageBoxW(Some(hwnd), t.pcwstr(), c.pcwstr(), MB_YESNO | MB_ICONQUESTION) == IDYES }
}

/// WSLが入っているか(`System32\wsl.exe`がある. K24: Copy WSL pathとWSLの一覧に使う)
pub fn wsl_installed() -> bool {
    std::path::PathBuf::from(super::storage::env("SystemRoot")).join(r"System32\wsl.exe").is_file()
}

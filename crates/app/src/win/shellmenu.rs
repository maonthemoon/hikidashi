//! ファイルの操作メニュー(→)の, Windowsのシェルに頼る3つ: プログラムから開く, 共有, Outlookの新しいメールに添付.

use super::util::{from_wide, wide, WStr};
use windows::core::{PCSTR, PCWSTR, PSTR, PWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IDataObject};
use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_ROUTINE_FLAGS, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY};
use windows::Win32::UI::Shell::{
    AssocQueryStringW, IAssocHandler, IContextMenu, IShellItem, SHAssocEnumHandlers, SHCreateItemFromParsingName, SHOpenWithDialog, ASSOCF_NONE, ASSOCSTR_FRIENDLYAPPNAME,
    ASSOC_FILTER_RECOMMENDED, BHID_DataObject, BHID_SFUIObject, CMF_NORMAL, CMINVOKECOMMANDINFO, GCS_VERBW, OAIF_EXEC, OPENASINFO,
};
use windows::Win32::UI::WindowsAndMessaging::{CreatePopupMenu, DestroyMenu, SW_SHOWNORMAL};

/// 「プログラムから開く」の1件
pub struct Handler {
    pub name: String,
    pub handler: IAssocHandler,
}

/// COMが返した文字列を取り出して解放する
unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

/// 拡張子(`.pdf`など)に関連付いたおすすめのアプリ. 既定のアプリを先頭にする
pub fn open_with_handlers(path: &str) -> Result<Vec<Handler>, String> {
    let ext = std::path::Path::new(path).extension().map(|e| format!(".{}", e.to_string_lossy())).ok_or("This file has no extension")?;
    let e = WStr::new(&ext);
    let mut out: Vec<Handler> = Vec::new();
    unsafe {
        let en = SHAssocEnumHandlers(e.pcwstr(), ASSOC_FILTER_RECOMMENDED).map_err(|er| format!("Could not list apps for {ext}: {er}"))?;
        loop {
            let mut buf: [Option<IAssocHandler>; 1] = [None];
            let mut got = 0u32;
            if en.Next(&mut buf, Some(&mut got)).is_err() || got == 0 {
                break;
            }
            let Some(h) = buf[0].take() else { break };
            let name = take_pwstr(h.GetUIName().unwrap_or(PWSTR::null()));
            if name.is_empty() {
                continue;
            }
            // おすすめだけ並べる(IsRecommendedはS_OKのとき0)
            if h.IsRecommended().0 != 0 {
                continue;
            }
            out.push(Handler { name, handler: h });
        }
    }
    // 既定のアプリ(名前で比べる)を先頭へ
    if let Some(def) = default_app_name(&ext) {
        if let Some(i) = out.iter().position(|h| h.name.eq_ignore_ascii_case(&def)) {
            let h = out.remove(i);
            out.insert(0, h);
        }
    }
    Ok(out)
}

fn default_app_name(ext: &str) -> Option<String> {
    let e = WStr::new(ext);
    let mut buf = [0u16; 260];
    let mut n = buf.len() as u32;
    let r = unsafe { AssocQueryStringW(ASSOCF_NONE, ASSOCSTR_FRIENDLYAPPNAME, e.pcwstr(), PCWSTR::null(), Some(PWSTR(buf.as_mut_ptr())), &mut n) };
    if r.is_ok() {
        Some(from_wide(&buf))
    } else {
        None
    }
}

/// 選んだアプリでファイルを開く. `Invoke`にはファイルの`IDataObject`を渡す
pub fn invoke_handler(h: &IAssocHandler, path: &str) -> Result<(), String> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(WStr::new(path).pcwstr(), None::<&IBindCtx>).map_err(|e| format!("Could not open {path}: {e}"))?;
        let data: IDataObject = item.BindToHandler(None::<&IBindCtx>, &BHID_DataObject).map_err(|e| format!("Could not read {path}: {e}"))?;
        h.Invoke(&data).map_err(|e| format!("Could not open with the app: {e}"))
    }
}

/// Windowsの「別のアプリを選ぶ」画面で開く
pub fn choose_another_app(hwnd: HWND, path: &str) -> Result<(), String> {
    let p = wide(path);
    let info = OPENASINFO { pcszFile: PCWSTR(p.as_ptr()), pcszClass: PCWSTR::null(), oaifInFlags: OAIF_EXEC };
    unsafe { SHOpenWithDialog(Some(hwnd), &info) }.map_err(|e| format!("Could not show the app list: {e}"))
}

/// Windowsの共有のウィンドウでファイルを共有する.
/// 方法: ファイルの右クリックのメニュー(`IContextMenu`)から`Windows.ModernShare`の動詞を探して呼ぶ.
/// 理由: `IDataTransferManagerInterop::ShowShareUIForWindow`と`DataRequested`で`StorageFile`を渡す方法は
/// WinRTの非同期とイベントの受け口が要り, 呼んだあとにHikidashiのウィンドウが隠れると
/// 共有の画面を出せなくなるおそれがある. 右クリックのメニューの動詞なら, Explorerと同じ経路で
/// 共有の画面がHikidashiとは別に出る(Windows 10, 11どちらでも同じ動詞).
pub fn share(hwnd: HWND, path: &str) -> Result<(), String> {
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(WStr::new(path).pcwstr(), None::<&IBindCtx>).map_err(|e| format!("Could not open {path}: {e}"))?;
        let cm: IContextMenu = item.BindToHandler(None::<&IBindCtx>, &BHID_SFUIObject).map_err(|e| format!("Could not read the menu of {path}: {e}"))?;
        let menu = CreatePopupMenu().map_err(|e| format!("Could not make a menu: {e}"))?;
        let r = (|| {
            let hr = cm.QueryContextMenu(menu, 0, 1, 0x7FFF, CMF_NORMAL);
            if hr.is_err() {
                return Err(format!("Could not read the menu of {path}: {hr:?}"));
            }
            // 戻り値の下位16ビットは, 使ったコマンド番号の最大+1
            let count = (hr.0 as u32 & 0xFFFF) as usize;
            for id in 0..count {
                let mut buf = [0u16; 128];
                // `GCS_VERBW`のときは, 渡した`PSTR`を実際には`PWSTR`として扱う
                let p = PSTR(buf.as_mut_ptr() as *mut u8);
                if cm.GetCommandString(id + 1, GCS_VERBW, None, p, buf.len() as u32).is_err() {
                    continue;
                }
                if !from_wide(&buf).eq_ignore_ascii_case("Windows.ModernShare") {
                    continue;
                }
                let info = CMINVOKECOMMANDINFO {
                    cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                    hwnd,
                    lpVerb: PCSTR((id + 1) as *const u8),
                    nShow: SW_SHOWNORMAL.0,
                    ..Default::default()
                };
                return cm.InvokeCommand(&info).map_err(|e| format!("Could not share {path}: {e}"));
            }
            Err("Sharing is not available for this file".to_string())
        })();
        let _ = DestroyMenu(menu);
        r
    }
}

/// `App Paths\OUTLOOK.EXE`の既定値(HKLMとHKCU, 32ビット側も)からOutlookのexeを探す
pub fn outlook_exe() -> Option<String> {
    let key = WStr::new(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\OUTLOOK.EXE");
    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for wow in [0, RRF_SUBKEY_WOW6432KEY.0] {
            if let Some(s) = reg_default(root, &key, wow) {
                let p = s.trim().trim_matches('"').to_string();
                if !p.is_empty() && std::path::Path::new(&p).is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

fn reg_default(root: HKEY, key: &WStr, wow: u32) -> Option<String> {
    let flags = REG_ROUTINE_FLAGS(RRF_RT_REG_SZ.0 | RRF_RT_REG_EXPAND_SZ.0 | wow);
    let mut buf = [0u16; 520];
    let mut size = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(root, key.pcwstr(), PCWSTR::null(), flags, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size)) };
    if r.is_ok() {
        Some(from_wide(&buf))
    } else {
        None
    }
}

/// Outlookの新しいメールにファイルを付けて開く(`outlook.exe /a "<パス>"`)
pub fn attach_to_new_mail(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let exe = outlook_exe().ok_or("Outlook was not found")?;
    super::storage::log(&format!("attach to new mail: {exe} {path}"));
    std::process::Command::new(&exe).raw_arg(format!("/a \"{path}\"")).spawn().map(|_| ()).map_err(|e| format!("Could not start Outlook: {e}"))
}

//! アプリの一覧(C1): スタートメニューのショートカット, デスクトップのアプリのショートカット(K30), ストアアプリ, 設定で足したフォルダーのexe.
//! スタートアップ, 説明書, Webページ, インストーラー, フォルダーへのショートカットは入れない(K30. `hikidashi_core::appfilter`).
//! `.lnk`の読み取りは更新日時でキャッシュする(J1).

use super::storage::{env, expand};
use super::util::{from_wide, WStr};
use hikidashi_core::appfilter;
use hikidashi_core::catalog::{Item, Kind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use windows::core::{Interface, PCWSTR, PWSTR};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOLDERID_AppsFolder, IEnumShellItems, IShellItem, IShellLinkW, SHGetKnownFolderItem, ShellLink, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING,
};

/// `.lnk`の読み取りのキャッシュ: パス → (更新日時, リンク先)
#[derive(Default)]
pub struct LnkCache {
    map: HashMap<PathBuf, (SystemTime, String)>,
}

impl LnkCache {
    fn target(&mut self, lnk: &Path) -> String {
        let mtime = std::fs::metadata(lnk).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        if let Some((t, target)) = self.map.get(lnk) {
            if *t == mtime {
                return target.clone();
            }
        }
        let target = resolve_lnk(lnk).unwrap_or_default();
        self.map.insert(lnk.to_path_buf(), (mtime, target.clone()));
        target
    }
}

/// `.lnk`のリンク先(exeのパス). 探しに行かない(遅いので)
fn resolve_lnk(lnk: &Path) -> Option<String> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let p = WStr::new(&lnk.to_string_lossy());
        pf.Load(p.pcwstr(), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        // SLGP_RAWPATH(4)は環境変数を残すので, 0(普通のパス)にする
        let _ = link.GetPath(&mut buf, std::ptr::null_mut(), 0);
        let s = from_wide(&buf);
        // MSIで入れたOfficeなどのショートカットは, リンク先が空か`C:\Windows\Installer\…\wordicon.exe`になる.
        // そのときはMSIに本当のexe(WINWORD.EXE)を聞く
        if s.is_empty() || s.to_ascii_lowercase().contains("\\installer\\") {
            if let Some(t) = msi_target(lnk) {
                return Some(t);
            }
        }
        (!s.is_empty()).then_some(s)
    }
}

/// MSIの「広告されたショートカット」から, 入っているexeのパスを引く
fn msi_target(lnk: &Path) -> Option<String> {
    use windows::Win32::System::ApplicationInstallationAndServicing::{MsiGetComponentPathW, MsiGetShortcutTargetW, INSTALLSTATE_LOCAL};
    unsafe {
        let p = WStr::new(&lnk.to_string_lossy());
        let mut product = [0u16; 39];
        let mut component = [0u16; 39];
        if MsiGetShortcutTargetW(p.pcwstr(), Some(PWSTR(product.as_mut_ptr())), None, Some(PWSTR(component.as_mut_ptr()))) != 0 {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let state = MsiGetComponentPathW(PCWSTR(product.as_ptr()), PCWSTR(component.as_ptr()), Some(PWSTR(buf.as_mut_ptr())), Some(&mut len));
        if state != INSTALLSTATE_LOCAL {
            return None;
        }
        let s = from_wide(&buf);
        (!s.is_empty()).then_some(s)
    }
}

/// スタートメニューのフォルダー(自分用と全員用)
pub fn start_menu_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    let appdata = env("APPDATA");
    if !appdata.is_empty() {
        v.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    let pd = env("ProgramData");
    if !pd.is_empty() {
        v.push(PathBuf::from(pd).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    v
}

/// デスクトップ(自分用と全員用. OneDriveに移していてもその場所)
pub fn desktop_dirs() -> Vec<PathBuf> {
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_PublicDesktop, KNOWN_FOLDER_FLAG};
    let mut v = Vec::new();
    for id in [&FOLDERID_Desktop, &FOLDERID_PublicDesktop] {
        unsafe {
            if let Ok(p) = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None) {
                if let Ok(s) = p.to_string() {
                    v.push(PathBuf::from(s));
                }
                windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
            }
        }
    }
    v
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>, exts: &[&str]) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() && !ft.is_symlink() {
            if depth > 0 {
                walk(&p, depth - 1, out, exts);
            }
        } else if let Some(ext) = p.extension().and_then(|x| x.to_str()) {
            if exts.iter().any(|x| x.eq_ignore_ascii_case(ext)) {
                out.push(p);
            }
        }
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// アンインストーラーやヘルプなど, 名前で探す対象にしないショートカット
fn is_noise(name: &str) -> bool {
    appfilter::is_noise_name(name)
}

/// リンク先で外すもの(説明書, インストーラー, フォルダー. K30)
fn is_noise_target(target: &str) -> bool {
    if appfilter::is_noise_target(target) {
        return true;
    }
    // 拡張子のないリンク先はフォルダーのことがある(サンプルデータなど)
    let p = Path::new(target);
    !target.is_empty() && p.extension().is_none() && p.is_dir()
}

/// `.url`がWebページへのリンクか(読めなければ外さない)
fn is_web_url_file(f: &Path) -> bool {
    std::fs::read(f).map(|b| appfilter::url_file_is_web(&String::from_utf8_lossy(&b))).unwrap_or(false)
}

/// ストアアプリ(パッケージのアプリ)を`shell:AppsFolder`から取る
fn store_apps() -> Vec<Item> {
    let mut out = Vec::new();
    unsafe {
        let Ok(folder) = SHGetKnownFolderItem::<IShellItem>(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None) else { return out };
        let Ok(en) = folder.BindToHandler::<_, IEnumShellItems>(None, &BHID_EnumItems) else { return out };
        loop {
            let mut items: [Option<IShellItem>; 1] = [None];
            let mut fetched = 0u32;
            if en.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = items[0].take() else { break };
            let name = display_name(&item, SIGDN_NORMALDISPLAY);
            let parsing = display_name(&item, SIGDN_PARENTRELATIVEPARSING);
            // パッケージのアプリだけ(`発行元!App`の形). デスクトップのアプリはショートカットで拾う
            if name.is_empty() || !parsing.contains('!') || is_noise(&name) {
                continue;
            }
            let target = format!("shell:AppsFolder\\{parsing}");
            let mut it = Item::new(Kind::App, name.clone(), "App", target);
            it.args = "store".into();
            // ストア版のOfficeなどは, パッケージの中のexeの名前でも探せる(winwordでWord)
            if let Some(exe) = package_exe(&item, &parsing) {
                if !exe.eq_ignore_ascii_case(&name) {
                    it.keys.push(exe);
                }
            }
            out.push(it);
        }
    }
    out
}

/// パッケージのアプリの, マニフェスト(`AppxManifest.xml`)に書かれたexeの名前(拡張子なし).
/// `発行元!Word`の`Word`に当たる`<Application Id="Word" Executable="Office16\WINWORD.EXE">`を探す.
unsafe fn package_exe(item: &IShellItem, parsing: &str) -> Option<String> {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::UI::Shell::IShellItem2;
    // PKEY_AppUserModel_PackageInstallPath
    const KEY: PROPERTYKEY = PROPERTYKEY { fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3), pid: 15 };
    let item2: IShellItem2 = item.cast().ok()?;
    let p = item2.GetString(&KEY).ok()?;
    let dir = pwstr_to_string(p);
    CoTaskMemFree(Some(p.0 as *const _));
    let id = parsing.rsplit('!').next()?;
    let xml = std::fs::read_to_string(Path::new(&dir).join("AppxManifest.xml")).ok()?;
    hikidashi_core::appx::manifest_exe(&xml, id)
}

unsafe fn display_name(item: &IShellItem, kind: windows::Win32::UI::Shell::SIGDN) -> String {
    match item.GetDisplayName(kind) {
        Ok(p) => {
            let s = pwstr_to_string(p);
            CoTaskMemFree(Some(p.0 as *const _));
            s
        }
        Err(_) => String::new(),
    }
}

unsafe fn pwstr_to_string(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    p.to_string().unwrap_or_default()
}

/// アプリの一覧を作る. 同じ名前は1つにまとめる(自分用のスタートメニューを優先).
pub fn build(cache: &mut LnkCache, extra_dirs: &[String]) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for dir in start_menu_dirs() {
        let mut files = Vec::new();
        walk(&dir, 6, &mut files, &["lnk", "url", "appref-ms"]);
        for f in files {
            let name = stem(&f);
            if name.is_empty() || is_noise(&name) {
                continue;
            }
            // スタートアップの中はWindowsの起動時に動くもの(K30)
            if f.strip_prefix(&dir).is_ok_and(|rel| appfilter::in_startup(&rel.to_string_lossy())) {
                continue;
            }
            let key = name.to_lowercase();
            if seen.contains_key(&key) {
                continue;
            }
            let ext = f.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
            if ext == "url" && is_web_url_file(&f) {
                continue;
            }
            let target = if ext == "lnk" { cache.target(&f) } else { String::new() };
            // リンク先がアンインストーラー, 説明書, フォルダーなら除く
            if is_noise(&stem(Path::new(&target))) || is_noise_target(&target) {
                continue;
            }
            let mut it = Item::new(Kind::App, name.clone(), short_dir(&f, &dir), f.to_string_lossy());
            // exeの名前でも探せる(J6. winwordでWord)
            if !target.is_empty() {
                let exe = stem(Path::new(&target));
                if !exe.is_empty() && !exe.eq_ignore_ascii_case(&name) {
                    it.keys.push(exe);
                }
            }
            it.args = target;
            seen.insert(key, items.len());
            items.push(it);
        }
    }
    // デスクトップのショートカット(K30): リンク先がexeのものだけ(フォルダーや文書へのショートカットは外す)
    for dir in desktop_dirs() {
        let mut files = Vec::new();
        walk(&dir, 0, &mut files, &["lnk", "appref-ms"]);
        for f in files {
            let name = stem(&f);
            let key = name.to_lowercase();
            if name.is_empty() || is_noise(&name) || seen.contains_key(&key) {
                continue;
            }
            let is_lnk = f.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"));
            let target = if is_lnk { cache.target(&f) } else { String::new() };
            if is_lnk && !target.to_ascii_lowercase().ends_with(".exe") {
                continue;
            }
            if is_noise(&stem(Path::new(&target))) || is_noise_target(&target) {
                continue;
            }
            let mut it = Item::new(Kind::App, name.clone(), "Desktop", f.to_string_lossy());
            let exe = stem(Path::new(&target));
            if !exe.is_empty() && !exe.eq_ignore_ascii_case(&name) {
                it.keys.push(exe);
            }
            it.args = target;
            seen.insert(key, items.len());
            items.push(it);
        }
    }
    for it in store_apps() {
        let key = it.name.to_lowercase();
        if seen.contains_key(&key) {
            continue;
        }
        seen.insert(key, items.len());
        items.push(it);
    }
    for d in extra_dirs {
        let dir = PathBuf::from(expand(d));
        let mut files = Vec::new();
        walk(&dir, 2, &mut files, &["exe", "lnk", "bat", "cmd"]);
        for f in files {
            let name = stem(&f);
            let key = name.to_lowercase();
            if name.is_empty() || seen.contains_key(&key) || is_noise(&name) {
                continue;
            }
            let mut it = Item::new(Kind::App, name, f.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(), f.to_string_lossy());
            it.args = f.to_string_lossy().into_owned();
            seen.insert(key, items.len());
            items.push(it);
        }
    }
    // 実体のよく使うもの: 電卓などはストアアプリなので上で拾える. exe名の別名だけ足す
    for it in items.iter_mut() {
        let l = it.name.to_lowercase();
        let extra: &[&str] = match l.as_str() {
            "calculator" | "電卓" => &["calc"],
            "notepad" | "メモ帳" => &["notepad"],
            "snipping tool" | "切り取り & スケッチ" => &["snippingtool"],
            "paint" | "ペイント" => &["mspaint"],
            "command prompt" | "コマンド プロンプト" => &["cmd"],
            "file explorer" | "エクスプローラー" => &["explorer"],
            _ => &[],
        };
        for e in extra {
            if !it.keys.iter().any(|k| k.eq_ignore_ascii_case(e)) {
                it.keys.push(e.to_string());
            }
        }
    }
    items
}

/// 2行目に出す, スタートメニューの中のフォルダー名(例: `Microsoft Office`). 直下なら`App`.
fn short_dir(f: &Path, root: &Path) -> String {
    match f.parent().and_then(|p| p.strip_prefix(root).ok()) {
        Some(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().into_owned(),
        _ => "App".into(),
    }
}

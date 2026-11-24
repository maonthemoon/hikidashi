//! アイコン(K2): アプリ・ファイル・フォルダーはWindowsのアイコン(Explorerと同じ), ほかはWindowsのアイコン用の文字.
//! ふつうのファイルとフォルダーは拡張子だけで引く(ファイルを開かないので, Boxや共有フォルダーでも待たされない).
//! アプリやショートカットのようにパスごとに読むものは, 裏のスレッドで読む(K29. 会社PCでショートカットのアイコンを
//! 読むのに0.6〜2秒かかり, 入力の途中で画面が止まった). 読めるまでは文字のアイコンを出し, 読めたら描き直す.

use super::util::WStr;
use std::collections::{HashMap, HashSet};
use std::sync::{mpsc, Arc, Mutex};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleDC, DeleteDC, DeleteObject, GetObjectW, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAP, BLENDFUNCTION, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::UI::Controls::{IImageList, ILD_TRANSPARENT};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHGetImageList, SHFILEINFOW, SHGFI_SYSICONINDEX, SHGFI_USEFILEATTRIBUTES, SHIL_EXTRALARGE,
    SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, DrawIconEx, DI_NORMAL, HICON};

/// 種類ごとの決まったアイコン(Segoe Fluent Icons / Segoe MDL2 Assets の文字)
pub mod glyph {
    pub const BOOKMARK: char = '\u{E734}';
    pub const WINDOW: char = '\u{E8A7}';
    pub const KILL: char = '\u{E711}';
    pub const SETTING: char = '\u{E713}';
    pub const POWER: char = '\u{E7E8}';
    pub const CALC: char = '\u{E8EF}';
    pub const CHAT: char = '\u{E8BD}';
    pub const GLOBE: char = '\u{E774}';
    pub const COMMAND: char = '\u{E756}';
    pub const FOLDER: char = '\u{E8B7}';
    pub const FILE: char = '\u{E8A5}';
    pub const COPY: char = '\u{E8C8}';
    pub const OPEN: char = '\u{E8DA}';
    pub const CODE: char = '\u{E943}';
    pub const ADMIN: char = '\u{E7EF}';
    pub const LINK: char = '\u{E71B}';
}

/// フォルダーのアイコンを引くときの名前
pub const FOLDER_KEY: &str = ":folder";

#[derive(Clone, Copy)]
enum Img {
    Bitmap(isize),
    Icon(isize),
}

/// 裏で読んだアイコンを窓に知らせる
pub const WM_APP_ICONS: u32 = 0x8000 + 31;

/// 裏で読むアイコン: (覚える名前, パス, 大きさ)
type Req = (String, String, i32);
/// 読めたもの: (覚える名前, 大きさ, 絵)
type Done = (String, i32, Option<isize>);

pub struct Icons {
    cache: HashMap<String, Option<Img>>,
    size: i32,
    /// 裏で読んでいる名前
    pending: HashSet<String>,
    tx: Option<mpsc::Sender<Req>>,
    done: Arc<Mutex<Vec<Done>>>,
}

impl Icons {
    pub fn new(size: i32) -> Icons {
        Icons { cache: HashMap::new(), size, pending: HashSet::new(), tx: None, done: Arc::new(Mutex::new(vec![])) }
    }

    /// 裏で読むスレッドを始める. 読めたら`hwnd`に`WM_APP_ICONS`を送る
    pub fn start(&mut self, hwnd: windows::Win32::Foundation::HWND) {
        let (tx, rx) = mpsc::channel::<Req>();
        let done = self.done.clone();
        let h = hwnd.0 as isize;
        let ok = std::thread::Builder::new()
            .name("icons".into())
            .spawn(move || {
                unsafe {
                    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
                }
                for (key, path, size) in rx {
                    let t0 = std::time::Instant::now();
                    let img = load_by_path(&path, size);
                    if t0.elapsed().as_millis() >= 500 {
                        super::storage::log(&format!("icon: {} ms for {path}", t0.elapsed().as_millis()));
                    }
                    done.lock().unwrap().push((key, size, img));
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(windows::Win32::Foundation::HWND(h as *mut _)),
                            WM_APP_ICONS,
                            windows::Win32::Foundation::WPARAM(0),
                            windows::Win32::Foundation::LPARAM(0),
                        );
                    }
                }
            })
            .is_ok();
        if ok {
            self.tx = Some(tx);
        }
    }

    /// 裏で読めたものを受け取る(画面のスレッドで). 何か入ったらtrue(描き直す)
    pub fn take_loaded(&mut self) -> bool {
        let items: Vec<Done> = std::mem::take(&mut *self.done.lock().unwrap());
        let mut any = false;
        for (key, size, img) in items {
            self.pending.remove(&key);
            if size != self.size {
                // 拡大率が変わる前の大きさ. 使わない
                if let Some(h) = img {
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(h as *mut _));
                    }
                }
                continue;
            }
            self.cache.insert(key, img.map(Img::Bitmap));
            any = true;
        }
        any
    }

    /// 先に読んでおく(アプリの一覧ができたとき). 見るときに待たないように
    pub fn prefetch<'a>(&mut self, paths: impl Iterator<Item = &'a str>) {
        for p in paths {
            let key = Self::key(p);
            if !(key.starts_with('*') || key == FOLDER_KEY) {
                self.request(key, p);
            }
        }
    }

    fn request(&mut self, key: String, path: &str) {
        if self.cache.contains_key(&key) || self.pending.contains(&key) {
            return;
        }
        if let Some(tx) = &self.tx {
            if tx.send((key.clone(), path.to_string(), self.size)).is_ok() {
                self.pending.insert(key);
            }
        }
    }

    /// 画面の拡大率が変わったら作り直す
    pub fn set_size(&mut self, size: i32) {
        if size != self.size {
            self.clear();
            self.size = size;
        }
    }

    pub fn clear(&mut self) {
        // 裏で読んでいるものは, 大きさが違えば捨てる(`take_loaded`)
        self.pending.clear();
        for (_, v) in self.cache.drain() {
            unsafe {
                match v {
                    Some(Img::Bitmap(h)) => {
                        let _ = DeleteObject(HGDIOBJ(h as *mut _));
                    }
                    Some(Img::Icon(h)) => {
                        let _ = DestroyIcon(HICON(h as *mut _));
                    }
                    None => {}
                }
            }
        }
    }

    /// 覚えるときの名前. アプリやショートカットはパスごと, ふつうのファイルは拡張子ごと
    fn key(path: &str) -> String {
        if path == FOLDER_KEY {
            return path.into();
        }
        let l = path.to_lowercase();
        let name = l.rsplit(['\\', '/']).next().unwrap_or(&l);
        let ext = name.rsplit_once('.').map(|x| x.1).unwrap_or("");
        let per_path = l.starts_with("shell:") || ["exe", "lnk", "url", "ico", "appref-ms", "msc", "cpl"].contains(&ext);
        if per_path { l } else { format!("*.{ext}") }
    }

    /// 拡張子(またはフォルダー)のアイコンをシステムのイメージリストから引く
    fn load_by_type(&self, key: &str) -> Option<Img> {
        let _d = super::diag::doing(format!("icon {key}"));
        unsafe {
            let (name, attr) = if key == FOLDER_KEY { ("folder".to_string(), FILE_ATTRIBUTE_DIRECTORY) } else { (format!("x{}", &key[1..]), FILE_ATTRIBUTE_NORMAL) };
            let w = WStr::new(&name);
            let mut info = SHFILEINFOW::default();
            let r = SHGetFileInfoW(w.pcwstr(), attr, Some(&mut info), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES);
            if r == 0 {
                return None;
            }
            let list: IImageList = SHGetImageList(SHIL_EXTRALARGE as i32).ok()?;
            let icon = list.GetIcon(info.iIcon, ILD_TRANSPARENT.0).ok()?;
            Some(Img::Icon(icon.0 as isize))
        }
    }


    /// ブックマークのアイコン(favicon. PNG)を描く. 一度読んだものは覚える. 描けなければfalse
    pub fn draw_png(&mut self, hdc: HDC, url: &str, png: Option<&[u8]>, x: i32, y: i32) -> bool {
        let key = format!("fav:{url}");
        let img = match self.cache.get(&key) {
            Some(v) => *v,
            None => {
                let Some(png) = png else { return false };
                let v = decode_png(png, self.size).map(|b| Img::Bitmap(b.0 as isize));
                self.cache.insert(key, v);
                v
            }
        };
        match img {
            Some(i) => {
                self.blit(hdc, i, x, y);
                true
            }
            None => false,
        }
    }

    /// faviconを読み直したら, 覚えていたものを捨てる
    pub fn clear_favicons(&mut self) {
        let keys: Vec<String> = self.cache.keys().filter(|k| k.starts_with("fav:")).cloned().collect();
        for k in keys {
            if let Some(Some(Img::Bitmap(h))) = self.cache.remove(&k) {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(h as *mut _));
                }
            }
        }
    }

    /// アイコンを描く. 描けなければfalse(呼ぶ側が文字のアイコンを描く)
    pub fn draw(&mut self, hdc: HDC, path: &str, x: i32, y: i32) -> bool {
        let key = Self::key(path);
        let img = match self.cache.get(&key) {
            Some(v) => *v,
            None if key.starts_with('*') || key == FOLDER_KEY => {
                let v = self.load_by_type(&key);
                self.cache.insert(key, v);
                v
            }
            None => {
                // パスごとのものは裏で読む. 読めるまでは文字のアイコン
                self.request(key, path);
                return false;
            }
        };
        let Some(img) = img else { return false };
        self.blit(hdc, img, x, y);
        true
    }

    fn blit(&self, hdc: HDC, img: Img, x: i32, y: i32) {
        unsafe {
            match img {
                Img::Icon(h) => {
                    let _ = DrawIconEx(hdc, x, y, HICON(h as *mut _), self.size, self.size, 0, None, DI_NORMAL);
                }
                Img::Bitmap(h) => {
                    let bmp = HBITMAP(h as *mut _);
                    let mut info = BITMAP::default();
                    GetObjectW(bmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut _ as *mut _));
                    let mem = CreateCompatibleDC(Some(hdc));
                    let old = SelectObject(mem, bmp.into());
                    let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                    let _ = AlphaBlend(hdc, x, y, self.size, self.size, mem, 0, 0, info.bmWidth, info.bmHeight.abs(), bf);
                    SelectObject(mem, old);
                    let _ = DeleteDC(mem);
                }
            }
        }
    }
}

/// PNGを読み, `size`四方の, 透明の値を掛けた32ビットの絵にする(AlphaBlendで描ける形). Windows標準のWICを使う
fn decode_png(png: &[u8], size: i32) -> Option<HBITMAP> {
    use windows::Win32::Graphics::Gdi::{CreateDIBSection, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS};
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    unsafe {
        let f: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = f.CreateStream().ok()?;
        stream.InitializeFromMemory(png).ok()?;
        let dec = f.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let scaler = f.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, size as u32, size as u32, WICBitmapInterpolationModeFant).ok()?;
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bmp.into());
            return None;
        }
        let stride = size as u32 * 4;
        let buf = std::slice::from_raw_parts_mut(bits as *mut u8, (stride * size as u32) as usize);
        if conv.CopyPixels(std::ptr::null(), stride, buf).is_err() {
            let _ = DeleteObject(bmp.into());
            return None;
        }
        Some(bmp)
    }
}

impl Drop for Icons {
    fn drop(&mut self) {
        self.clear();
    }
}

/// アプリなどのアイコン(パスのものを読む). 裏のスレッドで呼ぶ(時間がかかることがある)
fn load_by_path(path: &str, size: i32) -> Option<isize> {
    unsafe {
        let p = WStr::new(path);
        let f: IShellItemImageFactory = SHCreateItemFromParsingName(p.pcwstr(), None).ok()?;
        let bmp = f.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        Some(bmp.0 as isize)
    }
}

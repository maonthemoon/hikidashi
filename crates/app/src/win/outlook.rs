//! Outlookの予定(`cal `, K62). COMのlate binding(`IDispatch`)で, 裏のスレッドで読んで開く.
//! 絞り込みと行の文字は`hikidashi_core::calendar`.

use super::util::{from_wide, WStr};
use super::{storage, system};
use hikidashi_core::calendar::{self, Event};
use std::mem::ManuallyDrop;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use windows::core::{w, BSTR, GUID, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, SYSTEMTIME, VARIANT_TRUE, WPARAM};
use windows::Win32::Globalization::{GetDateFormatEx, GetTimeFormatEx, DATE_SHORTDATE, TIME_NOSECONDS};
use windows::Win32::System::Com::{
    CLSIDFromProgID, CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, DISPATCH_FLAGS,
    DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO,
};
use windows::Win32::System::Ole::DISPID_PROPERTYPUT;
use windows::Win32::System::Variant::{VariantChangeType, VariantClear, VAR_CHANGE_FLAGS, VARENUM, VARIANT, VT_BOOL, VT_BSTR, VT_DATE, VT_DISPATCH, VT_I4};
use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, PostMessageW, ASFW_ANY};

/// 予定を読み終えた
pub const WM_APP_CALENDAR: u32 = 0x8000 + 51;
/// 前に読んでからこれだけたったら, `cal `に入ったときに読み直す(秒)
const REFRESH_SECS: u64 = 5 * 60;
/// Outlookの予定表(`olFolderCalendar`)
const OL_FOLDER_CALENDAR: i32 = 9;
/// `LOCALE_USER_DEFAULT`
const LCID_USER: u32 = 0x0400;
/// 読む予定を探すときに見る件数の上限(範囲の外のものを飛ばしても, これで止める)
const MAX_SCAN: usize = 5000;

/// 読んだ結果の覚え
#[derive(Default)]
pub struct CalState {
    pub events: Option<Arc<Vec<Event>>>,
    pub fetched: Option<Instant>,
    pub running: bool,
    /// 読めなかったときの理由の短い文(読めたらNone)
    pub error: Option<String>,
}

pub type Cal = Arc<Mutex<CalState>>;

/// 前に読んでから5分以上たっていれば(初めてなら必ず), 裏で読む. 読み終えたら`hwnd`に`WM_APP_CALENDAR`を送る
pub fn ensure(cal: &Cal, hwnd: HWND) {
    {
        let mut g = cal.lock().unwrap();
        let stale = g.fetched.is_none_or(|t| t.elapsed().as_secs() >= REFRESH_SECS);
        if g.running || !stale {
            return;
        }
        g.running = true;
    }
    let (shared, h) = (cal.clone(), hwnd.0 as isize);
    let spawned = std::thread::Builder::new().name("outlook-read".into()).spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let t0 = Instant::now();
        let r = read(super::util::now_secs(), super::util::tz_offset_secs());
        unsafe { CoUninitialize() };
        let mut g = shared.lock().unwrap();
        g.running = false;
        g.fetched = Some(Instant::now());
        match r {
            Ok(v) => {
                storage::log(&format!("calendar: read {} appointments in {} ms", v.len(), t0.elapsed().as_millis()));
                g.events = Some(Arc::new(v));
                g.error = None;
            }
            Err(f) => {
                storage::log(&format!("calendar: {}: {}", f.reason, f.detail));
                g.events = None;
                g.error = Some(f.reason.to_string());
            }
        }
        drop(g);
        unsafe {
            let _ = PostMessageW(Some(HWND(h as *mut _)), WM_APP_CALENDAR, WPARAM(0), LPARAM(0));
        }
    });
    if let Err(e) = spawned {
        storage::log(&format!("calendar: could not start a thread: {e}"));
        cal.lock().unwrap().running = false;
    }
}

/// 予定をOutlookで開く(裏のスレッドで). 繰り返しの予定はシリーズが開く
pub fn open(entry_id: &str) {
    // Outlookの窓が前に出られるように(今はこちらが前にいる)
    unsafe {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    }
    let id = entry_id.to_string();
    let _ = std::thread::Builder::new().name("outlook-open".into()).spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        let r = display(&id);
        unsafe { CoUninitialize() };
        if let Err(f) = r {
            storage::log(&format!("calendar: open failed: {}: {}", f.reason, f.detail));
            system::notice(&format!("Cannot open the appointment in Outlook: {}", f.reason));
        }
    });
}

/// 失敗: `reason`は画面に出す短い文(英語), `detail`はログに書く詳しい文
struct Fail {
    reason: &'static str,
    detail: String,
}

fn fail(reason: &'static str) -> impl FnOnce(String) -> Fail {
    move |detail| Fail { reason, detail }
}

/// Outlookにつなぐ(起動中ならそれに, なければ裏で起動する)
fn connect() -> Result<IDispatch, Fail> {
    let clsid = unsafe { CLSIDFromProgID(w!("Outlook.Application")) }.map_err(|e| Fail { reason: "Outlook is not installed", detail: format!("CLSIDFromProgID: {}", err_text(&e)) })?;
    unsafe { CoCreateInstance::<_, IDispatch>(&clsid, None, CLSCTX_LOCAL_SERVER) }.map_err(|e| Fail { reason: "Could not start or connect to Outlook", detail: format!("CoCreateInstance: {}", err_text(&e)) })
}

fn read(now: u64, tz: i64) -> Result<Vec<Event>, Fail> {
    let app = connect()?;
    let bad = fail("Could not read the calendar");
    read_items(&app, now, tz).map_err(bad)
}

fn read_items(app: &IDispatch, now: u64, tz: i64) -> Result<Vec<Event>, String> {
    let ns = obj(app, "GetNamespace", vec![Var::str("MAPI")])?;
    let folder = obj(&ns, "GetDefaultFolder", vec![Var::int(OL_FOLDER_CALENDAR)])?;
    let items = obj(&folder, "Items", vec![])?;
    // 繰り返しの予定を1回ずつ出すには, 始まりで並べてからIncludeRecurrencesにする(Outlookの決まり)
    call(&items, "Sort", vec![Var::str("[Start]")])?;
    put(&items, "IncludeRecurrences", Var::bool(true))?;
    let (from, to) = calendar::read_window(now, tz);
    let filter = format!("[Start] < '{}' AND [End] > '{}'", locale_datetime(to), locale_datetime(from));
    let found = obj(&items, "Restrict", vec![Var::str(&filter)]).map_err(|e| format!("{e} (filter: {filter})"))?;
    let mut out = vec![];
    let mut next = call(&found, "GetFirst", vec![])?.dispatch();
    let mut scanned = 0;
    while let Some(it) = next {
        scanned += 1;
        if out.len() >= calendar::MAX_EVENTS || scanned > MAX_SCAN {
            break;
        }
        match event_of(&it) {
            // 始まりで並んでいるので, 範囲の後ろに出たら終わり
            Ok(e) if e.start >= to => break,
            Ok(e) if e.end.max(e.start) <= from => {}
            Ok(e) => out.push(e),
            Err(e) => storage::log(&format!("calendar: skipped an item: {e}")),
        }
        next = call(&found, "GetNext", vec![])?.dispatch();
    }
    Ok(out)
}

fn event_of(it: &IDispatch) -> Result<Event, String> {
    let start = get(it, "Start")?.date().ok_or("Start is not a date")?;
    let end = get(it, "End")?.date().ok_or("End is not a date")?;
    Ok(Event {
        subject: get(it, "Subject")?.string(),
        location: get(it, "Location").map(|v| v.string()).unwrap_or_default(),
        start: calendar::from_ole_date(start),
        end: calendar::from_ole_date(end),
        all_day: get(it, "AllDayEvent").map(|v| v.truthy()).unwrap_or(false),
        entry_id: get(it, "EntryID")?.string(),
    })
}

fn display(entry_id: &str) -> Result<(), Fail> {
    let app = connect()?;
    let bad = fail("Could not open the appointment");
    (|| -> Result<(), String> {
        let ns = obj(&app, "GetNamespace", vec![Var::str("MAPI")])?;
        let item = obj(&ns, "GetItemFromID", vec![Var::str(entry_id)])?;
        call(&item, "Display", vec![])?;
        // 開いた窓を前に出す(できなくてもよい)
        if let Ok(ins) = obj(&item, "GetInspector", vec![]) {
            let _ = call(&ins, "Activate", vec![]);
        }
        Ok(())
    })()
    .map_err(bad)
}

/// `Restrict`に渡す日時: ユーザーの地域の短い日付と, 秒のない時刻(`2026/11/27 0:00`, `11/27/2026 12:00 AM`など)
fn locale_datetime(t: i64) -> String {
    let (y, m, d, h, mi) = calendar::parts(t);
    let st = SYSTEMTIME { wYear: y as u16, wMonth: m as u16, wDay: d as u16, wHour: h as u16, wMinute: mi as u16, ..Default::default() };
    let mut db = [0u16; 128];
    let mut tb = [0u16; 128];
    unsafe {
        GetDateFormatEx(PCWSTR::null(), DATE_SHORTDATE, Some(&st), PCWSTR::null(), Some(&mut db), PCWSTR::null());
        GetTimeFormatEx(PCWSTR::null(), TIME_NOSECONDS, Some(&st), PCWSTR::null(), Some(&mut tb));
    }
    format!("{} {}", from_wide(&db), from_wide(&tb))
}

fn err_text(e: &windows::core::Error) -> String {
    format!("{} (0x{:08X})", e.message().trim(), e.code().0 as u32)
}

// ---- IDispatchを包む小さな道具 ----

/// 持ち主のいるVARIANT. 捨てるときにVariantClearする
#[repr(transparent)]
struct Var(VARIANT);

impl Drop for Var {
    fn drop(&mut self) {
        unsafe {
            let _ = VariantClear(&mut self.0);
        }
    }
}

impl Var {
    fn empty() -> Var {
        Var(VARIANT::default())
    }

    fn with(vt: VARENUM, set: impl FnOnce(&mut windows::Win32::System::Variant::VARIANT_0_0_0)) -> Var {
        let mut v = VARIANT::default();
        unsafe {
            let inner = &mut *v.Anonymous.Anonymous;
            inner.vt = vt;
            set(&mut inner.Anonymous);
        }
        Var(v)
    }

    fn int(n: i32) -> Var {
        Var::with(VT_I4, |a| a.lVal = n)
    }

    fn str(s: &str) -> Var {
        Var::with(VT_BSTR, |a| a.bstrVal = ManuallyDrop::new(BSTR::from(s)))
    }

    fn bool(b: bool) -> Var {
        Var::with(VT_BOOL, |a| a.boolVal = if b { VARIANT_TRUE } else { Default::default() })
    }

    fn vt(&self) -> VARENUM {
        unsafe { self.0.Anonymous.Anonymous.vt }
    }

    fn dispatch(&self) -> Option<IDispatch> {
        if self.vt() != VT_DISPATCH {
            return None;
        }
        unsafe { (*self.0.Anonymous.Anonymous.Anonymous.pdispVal).clone() }
    }

    /// 型を変えた写し. 変えられなければNone
    fn to(&self, vt: VARENUM) -> Option<Var> {
        let mut out = Var::empty();
        unsafe { VariantChangeType(&mut out.0, &self.0, VAR_CHANGE_FLAGS(0), vt) }.ok()?;
        Some(out)
    }

    /// 文字にする(空やNullは空の文字)
    fn string(&self) -> String {
        match self.to(VT_BSTR) {
            Some(v) => unsafe { v.0.Anonymous.Anonymous.Anonymous.bstrVal.to_string() },
            None => String::new(),
        }
    }

    /// OLEの日時
    fn date(&self) -> Option<f64> {
        self.to(VT_DATE).map(|v| unsafe { v.0.Anonymous.Anonymous.Anonymous.date })
    }

    fn truthy(&self) -> bool {
        self.to(VT_BOOL).is_some_and(|v| unsafe { v.0.Anonymous.Anonymous.Anonymous.boolVal.0 != 0 })
    }
}

/// 名前でメソッドを呼ぶかプロパティを読み書きする. `args`は左から順に書く
fn invoke(d: &IDispatch, name: &str, flags: DISPATCH_FLAGS, args: Vec<Var>) -> Result<Var, String> {
    let wname = WStr::new(name);
    let names = [wname.pcwstr()];
    let mut id = 0i32;
    unsafe { d.GetIDsOfNames(&GUID::zeroed(), names.as_ptr(), 1, LCID_USER, &mut id) }.map_err(|e| format!("{name}: no such member: {}", err_text(&e)))?;
    // IDispatchの引数は右から順に並べる. 中身の持ち主は`args`のまま(呼び終えたら捨てる)
    let mut raw: Vec<VARIANT> = args.iter().rev().map(|a| unsafe { std::ptr::read(&a.0) }).collect();
    let mut named = DISPID_PROPERTYPUT;
    let put = flags == DISPATCH_PROPERTYPUT;
    let dp = DISPPARAMS {
        rgvarg: if raw.is_empty() { std::ptr::null_mut() } else { raw.as_mut_ptr() },
        rgdispidNamedArgs: if put { &mut named } else { std::ptr::null_mut() },
        cArgs: raw.len() as u32,
        cNamedArgs: if put { 1 } else { 0 },
    };
    let mut result = Var::empty();
    let mut ex = EXCEPINFO::default();
    let r = unsafe { d.Invoke(id, &GUID::zeroed(), LCID_USER, flags, &dp, Some(&mut result.0), Some(&mut ex), None) };
    let (source, desc, help) = unsafe { (ManuallyDrop::take(&mut ex.bstrSource), ManuallyDrop::take(&mut ex.bstrDescription), ManuallyDrop::take(&mut ex.bstrHelpFile)) };
    drop((source, help));
    drop(args);
    match r {
        Ok(()) => Ok(result),
        Err(e) if !desc.is_empty() => Err(format!("{name}: {} ({})", desc.to_string().trim(), err_text(&e))),
        Err(e) => Err(format!("{name}: {}", err_text(&e))),
    }
}

fn call(d: &IDispatch, name: &str, args: Vec<Var>) -> Result<Var, String> {
    invoke(d, name, DISPATCH_METHOD | DISPATCH_PROPERTYGET, args)
}

fn get(d: &IDispatch, name: &str) -> Result<Var, String> {
    call(d, name, vec![])
}

fn put(d: &IDispatch, name: &str, v: Var) -> Result<(), String> {
    invoke(d, name, DISPATCH_PROPERTYPUT, vec![v]).map(|_| ())
}

/// 呼んだ結果のオブジェクト. オブジェクトでなければエラー
fn obj(d: &IDispatch, name: &str, args: Vec<Var>) -> Result<IDispatch, String> {
    call(d, name, args)?.dispatch().ok_or_else(|| format!("{name}: returned no object"))
}

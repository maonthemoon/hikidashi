//! ファイルの検索を別スレッドで行う. 新しい入力が来たら古い検索をやめる(世代番号. J1).

use hikidashi_core::files::{FileHit, FileTable};
use hikidashi_core::migemo::Dict;
use hikidashi_core::search::Prepared;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_APP_FILES: u32 = 0x8000 + 20;

pub struct Job {
    pub gen: u64,
    pub text: String,
    pub dirs_only: bool,
    pub tables: Vec<Arc<FileTable>>,
}

/// 検索の結果. `tables`は検索に使った索引(番号の意味がずれないように一緒に持つ)
pub struct Done {
    pub gen: u64,
    pub hits: Vec<(usize, FileHit)>,
    pub tables: Vec<Arc<FileTable>>,
    pub elapsed_ms: f64,
}

pub struct Searcher {
    tx: Sender<Job>,
    cancel: Mutex<Arc<AtomicBool>>,
    pub result: Arc<Mutex<Option<Done>>>,
    cancel_slot: Arc<Mutex<Arc<AtomicBool>>>,
}

#[derive(Clone, Copy)]
struct Hwnd(isize);
unsafe impl Send for Hwnd {}

/// 1つの索引から取る件数
const PER_TABLE: usize = hikidashi_core::router::MAX_ROWS;

impl Searcher {
    pub fn new(hwnd: HWND, dict: Arc<OnceLock<Dict>>) -> Searcher {
        let (tx, rx) = channel::<Job>();
        let result = Arc::new(Mutex::new(None));
        let cancel_slot = Arc::new(Mutex::new(Arc::new(AtomicBool::new(false))));
        let r2 = result.clone();
        let c2 = cancel_slot.clone();
        let h = Hwnd(hwnd.0 as isize);
        let _ = std::thread::Builder::new().name("searcher".into()).spawn(move || worker(rx, r2, c2, h, dict));
        Searcher { tx, cancel: Mutex::new(Arc::new(AtomicBool::new(false))), result, cancel_slot }
    }

    /// 新しい検索を頼む. 前の検索はやめる
    pub fn submit(&self, job: Job) {
        let old = self.cancel.lock().unwrap().clone();
        old.store(true, Ordering::Relaxed);
        let fresh = Arc::new(AtomicBool::new(false));
        *self.cancel.lock().unwrap() = fresh.clone();
        *self.cancel_slot.lock().unwrap() = fresh;
        let _ = self.tx.send(job);
    }

    pub fn cancel(&self) {
        self.cancel.lock().unwrap().store(true, Ordering::Relaxed);
    }
}

fn worker(rx: Receiver<Job>, result: Arc<Mutex<Option<Done>>>, cancel_slot: Arc<Mutex<Arc<AtomicBool>>>, hwnd: Hwnd, dict: Arc<OnceLock<Dict>>) {
    while let Ok(mut job) = rx.recv() {
        // たまっていたら一番新しいものだけを行う
        while let Ok(j) = rx.try_recv() {
            job = j;
        }
        let cancel = cancel_slot.lock().unwrap().clone();
        let t0 = Instant::now();
        let p = Prepared::new(&job.text, dict.get());
        let mut hits = Vec::new();
        for (i, t) in job.tables.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            for h in t.search_opt(&p, PER_TABLE, &cancel, job.dirs_only) {
                hits.push((i, h));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            continue;
        }
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        *result.lock().unwrap() = Some(Done { gen: job.gen, hits, tables: job.tables, elapsed_ms });
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd.0 as *mut _)), WM_APP_FILES, WPARAM(job.gen as usize), LPARAM(0));
        }
    }
}

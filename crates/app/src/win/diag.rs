//! 画面が止まったときの見張り(K29). 入力の途中で固まることがある, という会社PCでの指摘から.
//! 裏のスレッドが0.2秒ごとに窓へ合図を送り(PostMessage), 画面のスレッドが返事をするまでの時間を測る.
//! 0.3秒以上返事がなければ, そのとき画面のスレッドが何をしていたか(`doing`で書いたもの)を覚えておき,
//! 返事が来たら`hikidashi.log`に`ui: no response for 1234 ms (doing: icon C:\…)`と書く.
//! 打った文字はログに書かない(長さだけ).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub const WM_APP_PING: u32 = 0x8000 + 30;

static DOING: Mutex<String> = Mutex::new(String::new());
static PONG: AtomicU64 = AtomicU64::new(0);

/// 画面のスレッドがこれから行うこと. 返したものを捨てると元に戻る(入れ子にできる)
pub fn doing(what: impl Into<String>) -> Doing {
    let mut d = DOING.lock().unwrap();
    let prev = std::mem::replace(&mut *d, what.into());
    Doing { prev }
}

pub struct Doing {
    prev: String,
}

impl Drop for Doing {
    fn drop(&mut self) {
        *DOING.lock().unwrap() = std::mem::take(&mut self.prev);
    }
}

/// 合図への返事(画面のスレッドで, 窓が`WM_APP_PING`を受けたとき)
pub fn pong(seq: u64) {
    PONG.store(seq, Ordering::Relaxed);
}

/// 見張りを始める
pub fn start(hwnd: HWND) {
    let h = hwnd.0 as isize;
    std::thread::Builder::new()
        .name("ui-watch".into())
        .spawn(move || {
            let mut seq = 0u64;
            let mut sent = Instant::now();
            let mut seen: Option<String> = None;
            loop {
                std::thread::sleep(Duration::from_millis(100));
                let answered = PONG.load(Ordering::Relaxed) >= seq;
                if answered {
                    if let Some(what) = seen.take() {
                        let ms = sent.elapsed().as_millis();
                        super::storage::log(&format!("ui: no response for {ms} ms (doing: {})", if what.is_empty() { "unknown" } else { &what }));
                    }
                    if sent.elapsed() < Duration::from_millis(200) {
                        continue;
                    }
                    seq += 1;
                    sent = Instant::now();
                    unsafe {
                        if PostMessageW(Some(HWND(h as *mut _)), WM_APP_PING, WPARAM(seq as usize), LPARAM(0)).is_err() {
                            return;
                        }
                    }
                } else if seen.is_none() && sent.elapsed() >= Duration::from_millis(300) {
                    // 止まっている間に, 何をしていたかを覚える
                    seen = Some(DOING.lock().unwrap().clone());
                }
            }
        })
        .ok();
}

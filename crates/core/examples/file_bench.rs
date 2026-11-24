//! 50万件の名前で, 1文字ごとの検索の時間とメモリを測る. `cargo run --release --example file_bench`
use hikidashi_core::{files::FileTable, migemo, search::Prepared};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() {
    let words = ["report", "会議資料", "proposal", "budget", "定例", "design", "spec", "テスト", "image", "data", "schematic", "layout", "測定結果", "memo", "plan"];
    let exts = ["docx", "xlsx", "pptx", "pdf", "txt", "png", "csv", "rs"];
    let mut t = FileTable::new('\\');
    let root = t.push(None, r"C:\Users\me", true, 0);
    let mut dirs = vec![root];
    let mut seed: u64 = 1;
    let mut rnd = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
    let start = Instant::now();
    for i in 0..500_000u32 {
        let p = dirs[(rnd() as usize) % dirs.len()];
        if i % 20 == 0 {
            let id = t.push(Some(p), &format!("{}_{}", words[(rnd() % 15) as usize], i), true, 0);
            dirs.push(id);
        } else {
            let n = format!("{}_{}_{}.{}", words[(rnd() % 15) as usize], words[(rnd() % 15) as usize], i, exts[(rnd() % 8) as usize]);
            t.push(Some(p), &n, false, 0);
        }
    }
    t.shrink();
    println!("build: {} entries, {:?}, table memory {:.1} MB", t.len(), start.elapsed(), t.memory() as f64 / 1e6);
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../dev/dict/SKK-JISYO.L.utf8")).unwrap();
    let dict = migemo::Dict::parse(&src);
    let cancel = AtomicBool::new(false);
    for q in ["r", "re", "rep", "repo", "rpt", "kaigi", "kaigishiryou", "zzzz", "会議", "sch_lay"] {
        let t0 = Instant::now();
        let p = Prepared::new(q, Some(&dict));
        let h = t.search(&p, 50, &cancel);
        let el = t0.elapsed();
        println!("{q:>14}: {:>6.1} ms, top: {:?}", el.as_secs_f64() * 1000.0, h.first().map(|x| t.name(x.id)));
    }
}

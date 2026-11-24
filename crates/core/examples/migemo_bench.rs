//! 実物のSKK辞書で, 読み込みの時間と広げる時間を測る. `cargo run --release --example migemo_bench`
use hikidashi_core::migemo;
use std::time::Instant;

fn main() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../dev/dict/SKK-JISYO.L.utf8")).unwrap();
    let t = Instant::now();
    let d = migemo::Dict::parse(&src);
    drop(src);
    println!("parse: {} entries, {:?}", d.len(), t.elapsed());
    for q in ["kaigi", "kaig", "shiryou", "houkokusho", "teirei", "kak", "disupurei", "pwpt"] {
        let t = Instant::now();
        let e = migemo::expand(q, Some(&d));
        println!("{q}: {} words, {:?}, first: {:?}", e.words().len(), t.elapsed(), &e.words()[..e.words().len().min(8)]);
    }
}

use super::*;
use std::fs;
use std::time::Instant;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hikidashi-ext-test-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn rffe_host() -> ExtHost {
    ExtHost::start(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extensions"))
}

fn run(h: &ExtHost, kw: &str, q: &str, cb: Option<&str>) -> Vec<ExtRow> {
    h.search(kw, q, cb.map(|s| s.to_string()), next_gen()).recv().unwrap().rows
}

fn next_gen() -> u64 {
    use std::sync::atomic::AtomicU64;
    static G: AtomicU64 = AtomicU64::new(1);
    G.fetch_add(1, Ordering::SeqCst)
}

#[test]
fn 見本の拡張を読んで行が返る() {
    let h = rffe_host();
    assert!(h.errors().is_empty(), "{:?}", h.errors());
    let l = h.list();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].keyword, "rffe");
    assert_eq!(l[0].title, "MIPI RFFE");
    assert!(l[0].clipboard);
    let rows = run(&h, "rffe", "6 1C 38", None);
    assert_eq!(rows[0].title, "0110 010 11100 1 00111000 0");
    assert_eq!(rows[0].subtitle, "SA 0x6 · Register Write · 0x1C PM_TRIG ← 0x38 · Active, mask 111, trig 000");
    assert_eq!(rows[0].copy, "0110010111001001110000");
}

#[test]
fn rffeの値とビット列とパリティ() {
    let h = rffe_host();
    let cases = [
        (0x38, "Active, mask 111, trig 000"),
        (0x80, "Low Power, mask 000, trig 000"),
        (0x40, "Startup, mask 000, trig 000"),
        (0x01, "Active, mask 000, trig 001"),
        (0x02, "Active, mask 000, trig 010"),
        (0x04, "Active, mask 000, trig 100"),
        (0x07, "Active, mask 000, trig 111"),
        (0x00, "Active, mask 000, trig 000"),
    ];
    for (d, text) in cases {
        let rows = run(&h, "rffe", &format!("6 0x1C {:X}", d), None);
        let want = format!("SA 0x6 · Register Write · 0x1C PM_TRIG ← 0x{:02X} · {}", d, text);
        assert_eq!(rows[0].subtitle, want);
        // 値から作ったビット列を読み解くとパリティが合う
        let back = run(&h, "rffe", &rows[0].copy, None);
        assert_eq!(back[0].subtitle, format!("{} · parity OK", want));
        assert_eq!(back[0].copy, rows[0].copy);
        // 末尾のBPの0とスペース, _は無視
        let spaced = format!("{} 0", rows[0].title.replace(' ', "_"));
        let back2 = run(&h, "rffe", &spaced, None);
        assert_eq!(back2[0].subtitle, format!("{} · parity OK", want));
    }
    // パリティが違えばNG
    let rows = run(&h, "rffe", "0110010111001001110001", None);
    assert!(rows[0].subtitle.ends_with("parity NG"), "{}", rows[0].subtitle);
}

#[test]
fn rffeのほかの形() {
    let h = rffe_host();
    // Register 0 Write
    let r = run(&h, "rffe", "6 38", None);
    assert_eq!(r[0].title, "0110 10111000 1");
    assert!(r[0].subtitle.contains("Register 0 Write"), "{}", r[0].subtitle);
    let back = run(&h, "rffe", &r[0].copy, None);
    assert!(back[0].subtitle.ends_with("0x38 · parity OK"), "{}", back[0].subtitle);
    // Extended Register Write
    let r = run(&h, "rffe", "6 1C 123", None);
    assert!(r[0].subtitle.contains("Extended Register Write · 2 bytes"), "{}", r[0].subtitle);
    let back = run(&h, "rffe", &r[0].copy, None);
    assert!(back[0].subtitle.ends_with("← 0x01 0x23 · parity OK"), "{}", back[0].subtitle);
    let r = run(&h, "rffe", "6 40 12", None);
    assert!(r[0].subtitle.contains("Extended Register Write · 1 byte ·"), "{}", r[0].subtitle);
    // Long
    let r = run(&h, "rffe", "6 1234 AB", None);
    let back = run(&h, "rffe", &r[0].copy, None);
    assert!(back[0].subtitle.contains("Write Long · 1 byte · 0x1234 ← 0xAB · parity OK"), "{}", back[0].subtitle);
    // Register Read(データなし)
    let back = run(&h, "rffe", "0110011111001", None);
    assert!(back[0].subtitle.contains("Register Read · 0x1C PM_TRIG"), "{}", back[0].subtitle);
    // 分からないコマンド
    let back = run(&h, "rffe", "011000010000000000", None);
    assert!(back[0].subtitle.contains("unknown command"), "{}", back[0].subtitle);
    // 使い方
    let r = run(&h, "rffe", "", None);
    assert_eq!(r.len(), 1);
    assert!(r[0].title.contains("rffe 6 1C 38"));
}

fn frame_rows(byte: u8) -> Vec<u8> {
    // SA 6, Register Write, 0x1C, データ
    let mut bits = String::from("0110010111001");
    bits += &format!("{:08b}", byte);
    bits.push(if byte.count_ones() % 2 == 0 { '1' } else { '0' });
    bits.chars().map(|c| c as u8 - b'0').collect()
}

#[test]
fn rffeの表を読む() {
    let h = rffe_host();
    let a = frame_rows(0x38);
    let b = frame_rows(0x80);
    let mut t = String::from("Label\tSCLK\tACTIVE\tLOW_POW\tEmpty\n");
    for d in [1, 1, 0, 0] {
        t += &format!("x\t0\t{}\t{}\t0\n", d, d);
    }
    for i in 0..a.len() {
        t += &format!("x\t1\t{}\t{}\t0\n", a[i], b[i]);
        t += &format!("x\t0\t{}\t{}\t0\n", a[i], b[i]);
    }
    for _ in 0..2 {
        t += "x\t0\t0\t0\t0\n";
    }
    let rows = run(&h, "rffe", "", Some(&t));
    assert_eq!(rows.len(), 3, "{:?}", rows);
    assert_eq!(rows[0].title, "ACTIVE");
    assert_eq!(rows[0].subtitle, "SA 0x6 · Register Write · 0x1C PM_TRIG ← 0x38 · Active, mask 111, trig 000 · parity OK");
    assert_eq!(rows[1].title, "LOW_POW");
    assert_eq!(rows[1].subtitle, "SA 0x6 · Register Write · 0x1C PM_TRIG ← 0x80 · Low Power, mask 000, trig 000 · parity OK");
    assert_eq!(rows[2].title, "Empty");
    assert_eq!(rows[2].subtitle, "コマンドなし");
    // 見出しだけ大文字小文字が違っても読む. SSCがない列, ビットが足りない列
    let t2 = "sclk\tA\tB\n0\t1\t0\n1\t1\t0\n0\t1\t0\n1\t1\t0\n";
    let rows = run(&h, "rffe", "", Some(t2));
    assert!(rows[0].subtitle.contains("SSCが見つからない"), "{:?}", rows);
    assert_eq!(rows[1].subtitle, "コマンドなし");
    let t3 = "SCLK\tA\n0\t1\n0\t0\n1\t0\n0\t0\n1\t1\n0\t1\n";
    let rows = run(&h, "rffe", "", Some(t3));
    assert!(rows[0].subtitle.contains("ビットが足りない"), "{:?}", rows);
    // 表でないクリップボードは使い方
    let rows = run(&h, "rffe", "", Some("hello"));
    assert!(rows[0].title.contains("rffe 6 1C 38"));
}

#[test]
fn 例外はエラーの行() {
    let d = tmpdir("throw");
    fs::write(d.join("a.js"), r#"export const meta={title:"Bad",keyword:"bad"};
export function search(q,c){ if(q==="arr") return "str"; if(q==="ok") return [{title:"t"}]; throw new Error("boom\nsecond"); }"#).unwrap();
    let h = ExtHost::start(d);
    let r = run(&h, "bad", "x", None);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].title, "Bad: boom");
    let r = run(&h, "bad", "arr", None);
    assert!(r[0].title.starts_with("Bad: "), "{}", r[0].title);
    let r = run(&h, "bad", "ok", None);
    assert_eq!(r[0], ExtRow { title: "t".into(), subtitle: "".into(), copy: "t".into(), url: None });
    assert!(run(&h, "none", "x", None).is_empty());
}

#[test]
fn 行は50件まで() {
    let d = tmpdir("many");
    fs::write(d.join("m.js"), r#"export const meta={keyword:"m"};
export function search(q){ return Array.from({length:80},(_, i)=>({title:"r"+i, url:"https://example.com/"+i})); }"#).unwrap();
    let h = ExtHost::start(d);
    let r = run(&h, "m", "", None);
    assert_eq!(r.len(), 50);
    assert_eq!(r[1].url.as_deref(), Some("https://example.com/1"));
}

#[test]
fn 無限ループは止まって次が動く() {
    let d = tmpdir("loop");
    fs::write(d.join("l.js"), r#"export const meta={title:"Loop",keyword:"l"};
export function search(q){ if(q==="loop"){ while(true){} } return [{title:"fine "+q}]; }"#).unwrap();
    let h = ExtHost::start(d);
    let t = Instant::now();
    let r = run(&h, "l", "loop", None);
    let el = t.elapsed();
    assert!(r[0].title.starts_with("Loop: 時間切れ"), "{}", r[0].title);
    assert!(el >= Duration::from_millis(150) && el < Duration::from_millis(1000), "{:?}", el);
    let r = run(&h, "l", "next", None);
    assert_eq!(r[0].title, "fine next");
}

#[test]
fn メモリの使いすぎも止まる() {
    let d = tmpdir("mem");
    fs::write(d.join("m.js"), r#"export const meta={title:"Mem",keyword:"m"};
export function search(q){ if(q==="big"){ const a=[]; for(;;) a.push("x".repeat(1000000)+a.length); } return [{title:"ok"}]; }"#).unwrap();
    let h = ExtHost::start(d);
    let r = run(&h, "m", "big", None);
    assert!(r[0].title.starts_with("Mem: "), "{}", r[0].title);
    let r = run(&h, "m", "x", None);
    assert_eq!(r[0].title, "ok");
}

#[test]
fn metaのないファイルなどはエラー() {
    let d = tmpdir("nometa");
    fs::write(d.join("nometa.js"), "export function search(q){return []}").unwrap();
    fs::write(d.join("nosearch.js"), "export const meta={keyword:'a'};").unwrap();
    fs::write(d.join("syntax.js"), "export const meta = {").unwrap();
    fs::write(d.join("nokw.js"), "export const meta={}; export function search(){return []}").unwrap();
    fs::write(d.join("spin.js"), "export const meta={keyword:'s'}; while(true){}").unwrap();
    fs::write(d.join("good.js"), "export const meta={keyword:'g'}; export function search(){return []}").unwrap();
    let h = ExtHost::start(d);
    let errs = h.errors();
    let names: Vec<&str> = errs.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(names, vec!["nokw", "nometa", "nosearch", "spin", "syntax"], "{:?}", errs);
    assert!(errs[1].1.contains("meta"), "{:?}", errs);
    assert!(errs[3].1.contains("時間切れ"), "{:?}", errs);
    assert_eq!(h.list().len(), 1);
}

#[test]
fn reloadで変更が反映される() {
    let d = tmpdir("reload");
    let p = d.join("r.js");
    fs::write(&p, r#"export const meta={title:"R",keyword:"r"}; export function search(q){return [{title:"v1"}]}"#).unwrap();
    let h = ExtHost::start(d.clone());
    assert_eq!(run(&h, "r", "", None)[0].title, "v1");
    // 変えていなければ読み直さない(ここでは動作だけ確かめる)
    h.reload_wait();
    assert_eq!(run(&h, "r", "", None)[0].title, "v1");
    // 内容(長さ)を変える
    fs::write(&p, r#"export const meta={title:"R2",keyword:"r2"}; export function search(q){return [{title:"version two"}]}"#).unwrap();
    h.reload_wait();
    assert_eq!(h.list()[0].keyword, "r2");
    assert_eq!(run(&h, "r2", "", None)[0].title, "version two");
    assert!(run(&h, "r", "", None).is_empty());
    // 新しいファイル, 壊す, 消す
    fs::write(d.join("n.js"), "export const meta={keyword:'n'}; export function search(){return [{title:'new'}]}").unwrap();
    h.reload_wait();
    assert_eq!(h.list().len(), 2);
    fs::write(&p, "export const meta = {").unwrap();
    h.reload_wait();
    assert_eq!(h.list().len(), 1);
    assert_eq!(h.errors().len(), 1);
    fs::remove_file(&p).unwrap();
    h.reload_wait();
    assert!(h.errors().is_empty());
    assert_eq!(h.list().len(), 1);
}

#[test]
fn クリップボードはmetaで頼んだときだけ渡る() {
    let d = tmpdir("clip");
    fs::write(d.join("a.js"), "export const meta={keyword:'a',clipboard:true}; export function search(q,c){return [{title:'['+c.clipboard+']'}]}").unwrap();
    fs::write(d.join("b.js"), "export const meta={keyword:'b'}; export function search(q,c){return [{title:'['+c.clipboard+']'}]}").unwrap();
    let h = ExtHost::start(d);
    assert_eq!(run(&h, "a", "", Some("abc"))[0].title, "[abc]");
    assert_eq!(run(&h, "a", "", None)[0].title, "[]");
    assert_eq!(run(&h, "b", "", Some("abc"))[0].title, "[]");
}

#[test]
fn 溜まった古い要求は捨てて最新だけ実行する() {
    let d = tmpdir("gen");
    fs::write(d.join("s.js"), "export const meta={keyword:'s'}; export function search(q){ const t=Date.now(); while(Date.now()-t<60){} return [{title:q}]}").unwrap();
    let h = ExtHost::start(d);
    let rxs: Vec<_> = (1..=10u64).map(|g| (g, h.search("s", &format!("q{}", g), None, 1000 + g))).collect();
    let mut got = Vec::new();
    for (g, rx) in rxs {
        if let Ok(r) = rx.recv() {
            assert_eq!(r.generation, 1000 + g);
            got.push(r.rows[0].title.clone());
        }
    }
    assert_eq!(got.last().unwrap(), "q10");
    assert!(got.len() < 10, "{:?}", got);
}

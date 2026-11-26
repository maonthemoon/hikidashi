use super::*;
use std::fs;
use std::time::Instant;

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("hikidashi-ext-test-{}-{}", std::process::id(), tag));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn mipi_host() -> ExtHost {
    ExtHost::start(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extensions")])
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
    let h = mipi_host();
    assert!(h.errors().is_empty(), "{:?}", h.errors());
    let l = h.list();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].keyword, "mipi");
    assert_eq!(l[0].title, "MIPI RFFE");
    assert!(l[0].clipboard);
    let rows = run(&h, "mipi", "sa=6 addr=1C data=38 cmd=w", None);
    assert_eq!(rows[0].copy, "01100101110010011100000");
}

/// 帯の箱を`名前:ビット`で並べる
fn band(rows: &[ExtRow]) -> String {
    rows.iter().flat_map(|r| &r.band).map(|g| format!("{}{}:{}", if g.gap { "| " } else { "" }, g.label, g.bits)).collect::<Vec<_>>().join(" ")
}

fn titles(rows: &[ExtRow]) -> Vec<&str> {
    rows.iter().filter(|r| r.band.is_empty()).map(|r| r.title.as_str()).collect()
}

#[test]
fn mipiのビット列を分ける() {
    let h = mipi_host();
    let r = run(&h, "mipi", "0110 0101_1100 1001110 0000", None);
    assert_eq!(band(&r), "SSC: | SA:0110 CMD:010 ADDR:11100 P:1 | DATA:00111000 P:0 | B:0");
    assert_eq!(titles(&r), vec!["Register Write", "Command frame   SA 0x6 · Register Write · 0x1C PM_TRIG", "Data frame   0x38"]);
    assert_eq!(r[1].subtitle, "SA 0x6 · 0x1C PM_TRIG ← 0x38 · Active, mask 111, trig 000 · 23 bits");
    assert_eq!(r[2].subtitle, "0110 010 11100 1 · parity OK");
    assert!(r.iter().all(|x| !x.bad));
    // 厳密: 最後のBがない, 余る, Bが1
    let r = run(&h, "mipi", "0110010111001001110000", None);
    assert_eq!(r[2].title, "1 more bit (the last B)");
    assert!(r[2].bad);
    let r = run(&h, "mipi", "011001011100100111000000", None);
    assert_eq!(r[2].title, "1 extra bit");
    let r = run(&h, "mipi", "01100101110010011100001", None);
    assert_eq!(r[2].title, "B must be 0");
    // パリティが違う: 正しい値も出す
    let r = run(&h, "mipi", "01100101110010011100010", None);
    assert!(r[3].bad && r[3].subtitle.contains("parity NG · P should be 0"), "{r:?}");
    assert!(r[0].band.iter().any(|g| g.bad));
    let r = run(&h, "mipi", "01100101110000011100000", None);
    assert!(r[2].bad && r[2].subtitle.contains("P should be 1"), "{r:?}");
    // コマンドフレームに足りない
    let r = run(&h, "mipi", "0110010", None);
    assert_eq!(r[0].title, "6 more bits for the command frame");
    // 分からないコマンド
    let r = run(&h, "mipi", "01100001000000000000", None);
    assert!(titles(&r).contains(&"Unknown command"));
}

#[test]
fn mipiの値からビット列() {
    let h = mipi_host();
    let cases = [
        ("cmd=w0 sa=6 data=38", "SSC: | SA:0110 CMD:1 DATA:0111000 P:1 | B:0", "Register 0 Write"),
        ("cmd=w sa=6 addr=1C data=38", "SSC: | SA:0110 CMD:010 ADDR:11100 P:1 | DATA:00111000 P:0 | B:0", "Register Write"),
        // 読みはデータの前にもBが付く
        ("cmd=r sa=6 addr=1C data=38", "SSC: | SA:0110 CMD:011 ADDR:11100 P:0 | B:0 | DATA:00111000 P:0 | B:0", "Register Read"),
        ("cmd=r sa=6 addr=1C", "SSC: | SA:0110 CMD:011 ADDR:11100 P:0 | B:0", "Register Read"),
        ("cmd=ew sa=6 addr=40 data=12,34", "SSC: | SA:0110 CMD:0000 BC:0001 P:0 | ADDRESS:01000000 P:0 | DATA:00010010 P:1 | DATA:00110100 P:0 | B:0", "Extended Register Write"),
        ("cmd=er sa=6 addr=40 bytes=2", "SSC: | SA:0110 CMD:0010 BC:0001 P:1 | ADDRESS:01000000 P:0 | B:0", "Extended Register Read"),
        ("cmd=er sa=6 addr=40 data=1234", "SSC: | SA:0110 CMD:0010 BC:0001 P:1 | ADDRESS:01000000 P:0 | B:0 | DATA:00010010 P:1 | DATA:00110100 P:0 | B:0", "Extended Register Read"),
        ("cmd=ewl sa=6 addr=1234 data=AB", "SSC: | SA:0110 CMD:00110 BC:000 P:1 | ADDRESS:00010010 P:1 | ADDRESS:00110100 P:0 | DATA:10101011 P:0 | B:0", "Extended Register Write Long"),
        ("cmd=erl sa=6 addr=1234 bytes=2", "SSC: | SA:0110 CMD:00111 BC:001 P:1 | ADDRESS:00010010 P:1 | ADDRESS:00110100 P:0 | B:0", "Extended Register Read Long"),
    ];
    for (q, want, mode) in cases {
        let r = run(&h, "mipi", q, None);
        assert_eq!(band(&r), want, "{q}");
        assert!(r.iter().all(|x| !x.bad), "{q}: {r:?}");
        // 作ったビット列を読み解くと同じ分解になる
        let back = run(&h, "mipi", &r[0].copy, None);
        assert_eq!(band(&back), want, "{q}");
        assert_eq!(titles(&back)[0], mode, "{q}");
        // 順番は自由. 0xと小文字もよい
        let rev: Vec<&str> = q.split(' ').rev().collect();
        assert_eq!(run(&h, "mipi", &rev.join(" ").to_lowercase(), None)[0].copy, r[0].copy, "{q}");
    }
    // 種類を書かなければ, 使えるものを全部. Tabで種類を決められる
    let r = run(&h, "mipi", "sa=6 addr=1C data=0x38", None);
    assert_eq!(r.len(), 3);
    assert!(r[0].subtitle.starts_with("Register Write") && r[1].subtitle.starts_with("Extended Register Write ·") && r[2].subtitle.starts_with("Extended Register Write Long"));
    assert_eq!(r[0].complete.as_deref(), Some("mipi cmd=w sa=6 addr=1C data=0x38 "));
    assert_eq!(r[0].copy, "01100101110010011100000");
    // 範囲の外, 書き間違い
    for (q, e) in [("cmd=w sa=6 addr=40 data=12", "addr is up to 1F"), ("sa=16 addr=1", "sa is 0 to F"), ("sa=6 addr=XY", "addr=XY: hex only"), ("foo=1", "foo=1: use sa= addr= data= (hex)"), ("cmd=w0 sa=6 data=80", "data is 7 bits (up to 7F)"), ("cmd=w sa=6 addr=1 data=1234", "1 byte only")] {
        let r = run(&h, "mipi", q, None);
        assert!(r.iter().any(|x| x.title == e && x.bad), "{q}: {r:?}");
    }
    // データが多いときは帯を分ける
    let r = run(&h, "mipi", "cmd=ew sa=6 addr=40 data=000102030405060708090A0B0C0D0E0F", None);
    assert_eq!(r.iter().filter(|x| !x.band.is_empty()).count(), 4);
    assert_eq!(r.iter().filter(|x| x.title.starts_with("Data frame")).count(), 16);
}

#[test]
fn mipiの補完() {
    let h = mipi_host();
    let fills = |q: &str| run(&h, "mipi", q, None).into_iter().filter_map(|r| r.fill).collect::<Vec<_>>();
    // 何もなければコマンドの一覧. Masked Writeは設定がなければ出さない
    let f = fills("");
    assert_eq!(f.len(), 7);
    assert_eq!(f[1], "mipi cmd=w ");
    assert_eq!(fills("cmd=e"), vec!["mipi cmd=ew ", "mipi cmd=er ", "mipi cmd=ewl ", "mipi cmd=erl "]);
    assert_eq!(fills("cmd=w "), vec!["mipi cmd=w sa=", "mipi cmd=w addr=", "mipi cmd=w data="]);
    assert_eq!(fills("cmd=w sa=6 "), vec!["mipi cmd=w sa=6 addr=", "mipi cmd=w sa=6 data="]);
    assert_eq!(fills("cmd=w sa=6 a"), vec!["mipi cmd=w sa=6 addr="]);
    // 値を打った直後でも, Tabで次の名前へ進める
    assert_eq!(fills("cmd=w sa=6"), vec!["mipi cmd=w sa=6 addr=", "mipi cmd=w sa=6 data="]);
    assert_eq!(fills("cmd=w sa=6 addr=1C data=3"), Vec::<String>::new());
    assert_eq!(fills("cmd=w sa=6 addr=1"), vec!["mipi cmd=w sa=6 addr=1C ", "mipi cmd=w sa=6 addr=1D ", "mipi cmd=w sa=6 addr=1E ", "mipi cmd=w sa=6 addr=1F "]);
    assert!(fills("cmd=er sa=6 addr=40 ").contains(&"mipi cmd=er sa=6 addr=40 bytes=".to_string()));
    assert!(fills("s").contains(&"mipi sa=".to_string()));
    // 補完だけで最後まで行ける
    let r = run(&h, "mipi", "cmd=w sa=6 addr=1C data=38 ", None);
    assert_eq!(r[0].copy, "01100101110010011100000");
    // クリップボードのビット列
    let r = run(&h, "mipi", "", Some("0110 0101 1100 1001 1100 000\r\n"));
    assert!(r[1].subtitle.starts_with("From the clipboard · SA 0x6"), "{r:?}");
}

#[test]
fn mipiの設定のコマンド() {
    let h = mipi_host();
    // 値は例(本当の値は公開されていないので持たない)
    let mut o = Options::new();
    o.entry("mipi".into()).or_default().insert("masked_write".into(), "0001 0101".into());
    assert!(run(&h, "mipi", "cmd=mw sa=6 addr=40 mask=F0 data=05", None)[0].bad);
    h.set_options(o);
    let r = run(&h, "mipi", "cmd=mw sa=6 addr=40 mask=F0 data=05", None);
    assert_eq!(band(&r), "SSC: | SA:0110 CMD:00010101 P:0 | ADDRESS:01000000 P:0 | MASK:11110000 P:1 | DATA:00000101 P:1 | B:0");
    let back = run(&h, "mipi", &r[0].copy, None);
    assert_eq!(titles(&back)[0], "Masked Write");
    assert!(back.iter().any(|x| x.title == "Mask frame   0xF0"));
}

fn frame_rows(byte: u8) -> Vec<u8> {
    // SA 6, Register Write, 0x1C, データ
    let mut bits = String::from("0110010111001");
    bits += &format!("{:08b}", byte);
    bits.push(if byte.count_ones() % 2 == 0 { '1' } else { '0' });
    bits.chars().map(|c| c as u8 - b'0').collect()
}

#[test]
fn mipiの表を読む() {
    let h = mipi_host();
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
    let rows = run(&h, "mipi", "", Some(&t));
    assert_eq!(rows.len(), 3, "{:?}", rows);
    assert_eq!(rows[0].title, "ACTIVE");
    assert_eq!(rows[0].subtitle, "Register Write · SA 0x6 · 0x1C PM_TRIG ← 0x38 · Active, mask 111, trig 000 · parity OK");
    assert_eq!(rows[1].title, "LOW_POW");
    assert_eq!(rows[1].subtitle, "Register Write · SA 0x6 · 0x1C PM_TRIG ← 0x80 · Low Power, mask 000, trig 000 · parity OK");
    assert_eq!(rows[2].title, "Empty");
    assert_eq!(rows[2].subtitle, "No command");
    // 見出しだけ大文字小文字が違っても読む. SSCがない列, ビットが足りない列
    let t2 = "sclk\tA\tB\n0\t1\t0\n1\t1\t0\n0\t1\t0\n1\t1\t0\n";
    let rows = run(&h, "mipi", "", Some(t2));
    assert!(rows[0].subtitle.contains("SSC not found"), "{:?}", rows);
    assert_eq!(rows[1].subtitle, "No command");
    let t3 = "SCLK\tA\n0\t1\n0\t0\n1\t0\n0\t0\n1\t1\n0\t1\n";
    let rows = run(&h, "mipi", "", Some(t3));
    assert!(rows[0].subtitle.contains("more bits for the command frame"), "{:?}", rows);
    // 表でないクリップボードは使い方
    let rows = run(&h, "mipi", "", Some("hello"));
    assert_eq!(rows[1].fill.as_deref(), Some("mipi cmd=w "));
}

#[test]
fn 例外はエラーの行() {
    let d = tmpdir("throw");
    fs::write(d.join("a.js"), r#"export const meta={title:"Bad",keyword:"bad"};
export function search(q,c){ if(q==="arr") return "str"; if(q==="ok") return [{title:"t"}]; throw new Error("boom\nsecond"); }"#).unwrap();
    let h = ExtHost::start(vec![d]);
    let r = run(&h, "bad", "x", None);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].title, "Bad: boom");
    let r = run(&h, "bad", "arr", None);
    assert!(r[0].title.starts_with("Bad: "), "{}", r[0].title);
    let r = run(&h, "bad", "ok", None);
    assert_eq!(r[0], ExtRow { title: "t".into(), subtitle: "".into(), copy: "t".into(), url: None, fill: None, complete: None, band: vec![], bad: false });
    assert!(run(&h, "none", "x", None).is_empty());
}

#[test]
fn 行は50件まで() {
    let d = tmpdir("many");
    fs::write(d.join("m.js"), r#"export const meta={keyword:"m"};
export function search(q){ return Array.from({length:80},(_, i)=>({title:"r"+i, url:"https://example.com/"+i})); }"#).unwrap();
    let h = ExtHost::start(vec![d]);
    let r = run(&h, "m", "", None);
    assert_eq!(r.len(), 50);
    assert_eq!(r[1].url.as_deref(), Some("https://example.com/1"));
}

#[test]
fn 無限ループは止まって次が動く() {
    let d = tmpdir("loop");
    fs::write(d.join("l.js"), r#"export const meta={title:"Loop",keyword:"l"};
export function search(q){ if(q==="loop"){ while(true){} } return [{title:"fine "+q}]; }"#).unwrap();
    let h = ExtHost::start(vec![d]);
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
    let h = ExtHost::start(vec![d]);
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
    let h = ExtHost::start(vec![d]);
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
    let h = ExtHost::start(vec![d.clone()]);
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
    let h = ExtHost::start(vec![d]);
    assert_eq!(run(&h, "a", "", Some("abc"))[0].title, "[abc]");
    assert_eq!(run(&h, "a", "", None)[0].title, "[]");
    assert_eq!(run(&h, "b", "", Some("abc"))[0].title, "[]");
}

#[test]
fn 溜まった古い要求は捨てて最新だけ実行する() {
    let d = tmpdir("gen");
    fs::write(d.join("s.js"), "export const meta={keyword:'s'}; export function search(q){ const t=Date.now(); while(Date.now()-t<60){} return [{title:q}]}").unwrap();
    let h = ExtHost::start(vec![d]);
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

#[test]
fn コマンドフレームのパリティはsaを含む() {
    let h = mipi_host();
    // SAの1の数が偶数(6)と奇数(7)で, コマンドフレームのPが変わる(ユーザーの資料: コマンドフレーム13ビットの1が奇数個)
    for (sa, p) in [("6", "1"), ("7", "0")] {
        let r = run(&h, "mipi", &format!("cmd=w sa={sa} addr=1C data=38"), None);
        assert_eq!(r[0].band[4].bits, p);
        assert!(run(&h, "mipi", &r[0].copy, None).iter().all(|x| !x.bad));
    }
}

#[test]
fn 同じファイル名は先のフォルダーを使う() {
    let a = tmpdir("prio-a");
    let b = tmpdir("prio-b");
    let missing = b.join("none");
    fs::write(a.join("x.js"), r#"export const meta={keyword:"x"};
export function search(){ return [{title:"user"}]; }"#).unwrap();
    fs::write(b.join("x.js"), r#"export const meta={keyword:"x"};
export function search(){ return [{title:"bundled"}]; }"#).unwrap();
    fs::write(b.join("y.js"), r#"export const meta={keyword:"y"};
export function search(){ return [{title:"only-b"}]; }"#).unwrap();
    // 先のフォルダーが優先. 存在しないフォルダーがあっても動く
    let h = ExtHost::start(vec![missing.clone(), a.clone(), b.clone()]);
    assert_eq!(h.list().len(), 2);
    assert_eq!(run(&h, "x", "", None)[0].title, "user");
    assert_eq!(run(&h, "y", "", None)[0].title, "only-b");
    // 順番を逆にすると同梱の方
    let h = ExtHost::start(vec![b, a]);
    assert_eq!(run(&h, "x", "", None)[0].title, "bundled");
}

#[test]
fn 計算のモードで使えるかはmetaで決まる() {
    let d = tmpdir("calcmeta");
    fs::write(d.join("a.js"), "export const meta={keyword:'a',calc:true}; export function search(){return []}").unwrap();
    fs::write(d.join("b.js"), "export const meta={keyword:'b'}; export function search(){return []}").unwrap();
    fs::write(d.join("mipi.js"), include_str!("../../../extensions/mipi.js")).unwrap();
    let h = ExtHost::start(vec![d]);
    let v: Vec<(String, bool)> = h.list().into_iter().map(|m| (m.keyword, m.calc)).collect();
    assert_eq!(v, vec![("a".into(), true), ("b".into(), false), ("mipi".into(), true)]);
}

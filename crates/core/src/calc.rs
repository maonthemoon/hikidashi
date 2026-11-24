//! 進数の変換, RFの計算, ふつうの計算(四則演算)(H5, K6). 結果の行は`Enter`でコピーする.

use std::f64::consts::PI;

/// 計算結果の1行. `value`は画面に出す文字, `copy`はコピーする文字, `note`は2行目の説明.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcRow {
    pub value: String,
    pub copy: String,
    pub note: String,
    /// 進数の変換のときの種類(`hex`, `dec`, `bin`)
    pub base: Option<&'static str>,
}

/// 計算の結果. 計算の行は`=`のモード(calc)のときだけ出す.
#[derive(Debug, Clone, PartialEq)]
pub struct Calc {
    pub rows: Vec<CalcRow>,
}

fn row(value: String, note: &str) -> CalcRow {
    CalcRow { copy: value.clone(), value, note: note.to_string(), base: None }
}

/// SI接頭辞を付けて有効数字4桁で書く(例: `150.8 Ω`, `10 mW`).
pub fn fmt_si(v: f64, unit: &str) -> String {
    if !v.is_finite() {
        return format!("{v} {unit}");
    }
    const PRE: [(f64, &str); 9] =
        [(1e12, "T"), (1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p")];
    if v == 0.0 {
        return format!("0 {unit}");
    }
    for (f, p) in PRE {
        if v.abs() >= f * 0.99995 {
            return format!("{} {}{}", sig4(v / f), p, unit);
        }
    }
    // 1 pより小さい値はpのまま小数で書く(0.4398 pF)
    format!("{} p{}", sig4(v / 1e-12), unit)
}

/// 有効数字4桁. 末尾の0は消す.
fn sig4(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let digits = 4 - 1 - v.abs().log10().floor() as i32;
    let s = if digits > 0 { format!("{:.*}", digits as usize, v) } else { format!("{:.0}", v) };
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

fn fixed2(v: f64) -> String {
    format!("{v:.2}")
}

/// 単位の前の接頭辞の倍率
fn si_prefix(p: &str) -> Option<f64> {
    Some(match p {
        "" => 1.0,
        "p" => 1e-12,
        "n" => 1e-9,
        "u" | "µ" | "μ" => 1e-6,
        "m" => 1e-3,
        "k" | "K" => 1e3,
        "M" => 1e6,
        "G" | "g" => 1e9,
        "T" => 1e12,
        _ => return None,
    })
}

/// `10nH`, `2.4GHz`, `-3dBm`のような「数 + 接頭辞 + 単位」を読む. 単位は大文字小文字を区別しない.
fn quantity(s: &str, unit: &str) -> Option<f64> {
    let s = s.trim();
    let lower = s.to_ascii_lowercase();
    let ul = unit.to_ascii_lowercase();
    if !lower.ends_with(&ul) {
        return None;
    }
    let body = s[..s.len() - unit.len()].trim_end();
    let num_end = body
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+'))))
        .map(|(i, _)| i)
        .unwrap_or(body.len());
    let num: f64 = body[..num_end].parse().ok()?;
    let pre = body[num_end..].trim();
    // "m"(ミリ)と"M"(メガ)は区別する. 周波数のmHzは使わないのでMHzとして扱う
    let f = if unit.eq_ignore_ascii_case("hz") && pre == "m" { 1e6 } else { si_prefix(pre)? };
    Some(num * f)
}

fn plain_number(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+') {
        return None;
    }
    s.parse().ok()
}

fn hex_row(v: u128, note: &str) -> CalcRow {
    let hex = format!("0x{v:X}");
    CalcRow { value: hex.clone(), copy: hex, note: note.into(), base: Some("hex") }
}

fn dec_row(v: u128, note: &str) -> CalcRow {
    CalcRow { value: v.to_string(), copy: v.to_string(), note: note.into(), base: Some("dec") }
}

/// 2進数は4桁ごとに区切って見せ, コピーは`0b`付きで区切りなし
fn bin_row(v: u128, note: &str) -> CalcRow {
    let mut bin = format!("{v:b}");
    while bin.len() % 4 != 0 {
        bin.insert(0, '0');
    }
    let grouped: Vec<&str> = (0..bin.len()).step_by(4).map(|i| &bin[i..i + 4]).collect();
    CalcRow { value: grouped.join(" "), copy: format!("0b{bin}"), note: note.into(), base: Some("bin") }
}

/// 進数の変換. 打った進数以外の2つを出す(2026-11-24. K15).
/// 書き方は3つだけ: 16進数は`0x7E8`, 2進数は`0b11111101000`, 10進数は`2024`(数字だけ).
/// - `0x`, `0b`の付いたもの: 16進数か2進数として読む.
/// - 数字だけ: 10進数として, 16進数と2進数を出す.
fn radix(s: &str) -> Option<Calc> {
    let raw = s.trim();
    if raw.contains(char::is_whitespace) {
        return None;
    }
    let t: String = raw.chars().filter(|c| *c != '_').collect();
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        let v = u128::from_str_radix(h, 16).ok()?;
        return Some(Calc { rows: vec![dec_row(v, "dec"), bin_row(v, "bin")] });
    }
    if let Some(b) = lower.strip_prefix("0b") {
        let v = u128::from_str_radix(b, 2).ok()?;
        return Some(Calc { rows: vec![hex_row(v, "hex"), dec_row(v, "dec")] });
    }
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        let d = t.parse::<u128>().ok()?;
        return Some(Calc { rows: vec![hex_row(d, "hex"), bin_row(d, "bin")] });
    }
    None
}

/// 周波数. `Hz`は省いてよい(`2.4G`, `2.4GHz`).
fn freq(s: &str) -> Option<f64> {
    let f = quantity(s, "Hz").or_else(|| quantity(&format!("{}Hz", s.trim()), "Hz"))?;
    (f > 0.0).then_some(f)
}

/// 直列共振: L, C, 周波数のうち2つから残りを出す. 区切りは空白, `,`, `&`. 順不同.
fn resonance(s: &str) -> Option<Calc> {
    let parts: Vec<&str> = s.split([' ', ',', '&']).filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        return None;
    }
    let (mut l, mut c, mut f) = (None, None, None);
    for p in parts {
        if let Some(v) = quantity(p, "H") {
            l = Some(v);
        } else if let Some(v) = quantity(p, "F") {
            c = Some(v);
        } else if let Some(v) = freq(p) {
            f = Some(v);
        } else {
            return None;
        }
    }
    let r = match (l, c, f) {
        (Some(l), Some(c), None) if l > 0.0 && c > 0.0 => row(fmt_si(1.0 / (2.0 * PI * (l * c).sqrt()), "Hz"), "Resonant frequency f0 = 1/(2π√LC)"),
        (Some(l), None, Some(f)) if l > 0.0 => row(fmt_si(1.0 / ((2.0 * PI * f).powi(2) * l), "F"), "Capacitance C = 1/(ω²L) for resonance"),
        (None, Some(c), Some(f)) if c > 0.0 => row(fmt_si(1.0 / ((2.0 * PI * f).powi(2) * c), "H"), "Inductance L = 1/(ω²C) for resonance"),
        _ => return None,
    };
    Some(Calc { rows: vec![r] })
}

fn reflection_rows(g: f64, rows: &mut Vec<CalcRow>, skip: &str) {
    let vswr = (1.0 + g) / (1.0 - g);
    let rl = -20.0 * g.max(1e-12).log10();
    let ml = -10.0 * (1.0 - g * g).max(1e-12).log10();
    if skip != "vswr" {
        rows.push(row(sig4(vswr), "VSWR"));
    }
    if skip != "rl" {
        rows.push(row(format!("{} dB", fixed2(rl)), "Return loss"));
    }
    if skip != "gamma" {
        rows.push(row(format!("{:.3}", g), "|Γ| reflection coefficient"));
    }
    rows.push(row(format!("{} %", fixed2(g * g * 100.0)), "Reflected power"));
    rows.push(row(format!("{} dB", sig4(ml)), "Mismatch loss"));
}

/// 基準インピーダンス`z0`にその電力をかけたときの電圧の振幅(ピーク). 2行目に実効値とピークtoピーク
fn voltage_row(w: f64, z0: f64) -> CalcRow {
    let vp = (2.0 * w * z0).sqrt();
    let note = format!("Peak voltage at Z0 = {} · Vrms {} · Vpp {}", fmt_si(z0, "Ω"), fmt_si(vp / 2f64.sqrt(), "V"), fmt_si(vp * 2.0, "V"));
    row(fmt_si(vp, "V"), &note)
}

/// `vswr 10 il 2dB`: 出力側の反射を, 挿入損失ILの部品を通して入力側から見た値.
/// 反射は行きと帰りでILを2回通るので, リターンロスが2×IL良くなる.
fn through_loss(lower: &str) -> Option<Calc> {
    let (left, right) = lower.split_once(" il")?;
    let il = plain_number(right.trim_start_matches([' ', '=', ':']).trim_end_matches("db").trim())?.abs();
    let left = left.trim();
    let mut g_out = None;
    for (k, kind) in [("vswr", 0), ("return loss", 1), ("rl", 1), ("gamma", 2), ("|γ|", 2), ("γ", 2), ("g", 2)] {
        if let Some(rest) = left.strip_prefix(k) {
            let v = plain_number(rest.trim_start_matches([' ', '=', ':']).trim_end_matches("db").trim())?;
            g_out = match kind {
                0 if v >= 1.0 => Some((v - 1.0) / (v + 1.0)),
                1 => Some(10f64.powf(-v.abs() / 20.0)),
                2 if (0.0..1.0).contains(&v) => Some(v),
                _ => None,
            };
            break;
        }
    }
    let g_out = g_out?;
    let vswr_out = (1.0 + g_out) / (1.0 - g_out);
    let rl_out = -20.0 * g_out.max(1e-12).log10();
    let g_in = g_out * 10f64.powf(-2.0 * il / 20.0);
    let rl_in = -20.0 * g_in.max(1e-12).log10();
    let vswr_in = (1.0 + g_in) / (1.0 - g_in);
    let rows = vec![
        row(sig4(vswr_in), &format!("VSWR at input · output VSWR {} (RL {} dB) through {} dB loss", sig4(vswr_out), fixed2(rl_out), sig4(il))),
        row(format!("{} dB", fixed2(rl_in)), "Return loss at input"),
        row(format!("{:.3}", g_in), "|Γ| at input"),
    ];
    Some(Calc { rows })
}

/// 複素数(実部, 虚部)
type Cx = (f64, f64);

fn cx_inv(a: Cx) -> Option<Cx> {
    let d = a.0 * a.0 + a.1 * a.1;
    (d > 0.0 && d.is_finite()).then(|| (a.0 / d, -a.1 / d))
}

/// 値が基準に比べて0とみなせるか
fn tiny(v: f64, scale: f64) -> bool {
    v.abs() <= scale.abs() * 1e-12
}

#[derive(Clone, Copy, PartialEq)]
enum Unit {
    None,
    Ohm,
    Siemens,
    H,
    F,
}

/// 式の1項(`50`, `j50`, `50j`, `10mS`, `3.979nH`など). `v`は符号と接頭辞を含む値.
struct Term {
    v: f64,
    j: bool,
    unit: Unit,
}

/// 末尾の単位を分ける. 返すのは(接頭辞の部分, 単位).
fn split_unit(s: &str) -> Option<(&str, Unit)> {
    let lower = s.to_ascii_lowercase();
    if lower.ends_with("ohm") {
        return Some((&s[..s.len() - 3], Unit::Ohm));
    }
    for ohm in ['\u{3a9}', '\u{2126}'] {
        if let Some(r) = s.strip_suffix(ohm) {
            return Some((r, Unit::Ohm));
        }
    }
    let unit = match lower.chars().last() {
        Some('s') => Unit::Siemens,
        Some('h') => Unit::H,
        Some('f') => Unit::F,
        _ => return Some((s, Unit::None)),
    };
    Some((&s[..s.len() - 1], unit))
}

fn parse_term(s: &str) -> Option<Term> {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut j = false;
    let mut s = s;
    if let Some(r) = s.strip_prefix(['j', 'J']) {
        j = true;
        s = r;
    }
    let end = s.char_indices().find(|(_, c)| !(c.is_ascii_digit() || *c == '.')).map(|(i, _)| i).unwrap_or(s.len());
    let num: f64 = s[..end].parse().ok()?;
    let mut rest = &s[end..];
    if !j {
        if let Some(r) = rest.strip_prefix(['j', 'J']) {
            j = true;
            rest = r;
        }
    }
    let (pre, unit) = split_unit(rest)?;
    let v = num * si_prefix(pre)?;
    Some(Term { v: if neg { -v } else { v }, j, unit })
}

/// 項に分ける(先頭以外の`+`と`-`で切る). 空白は取り除いた文字列を渡す.
fn split_terms(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if i > start && (c == '+' || c == '-') {
            out.push(&s[start..i]);
            start = i;
        }
    }
    out.push(&s[start..]);
    out
}

/// 入力の形(どの行を出すかが変わる)
#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// `50+j50`: インピーダンス
    Z,
    /// `10mS-j10mS`, `y 0.01-j0.01`: アドミタンス
    Y,
    /// `50+3.979nH`: 直列の等価回路
    Series,
    /// `100||7.958nH`: 並列の等価回路
    Parallel,
}

/// 直列の項の和. `admittance`ならアドミタンス(単位は`S`か無し), そうでなければインピーダンス(`Ω`か無しか素子).
/// 返すのは(複素数, 素子があったか, jか単位があったか).
fn sum_terms(s: &str, w: Option<f64>, admittance: bool) -> Option<(Cx, bool, bool)> {
    let (mut re, mut im) = (0.0, 0.0);
    let (mut elem, mut marked) = (false, false);
    for t in split_terms(s) {
        let t = parse_term(t)?;
        marked |= t.j || t.unit != Unit::None;
        match t.unit {
            Unit::H | Unit::F => {
                if admittance || t.j || t.v <= 0.0 {
                    return None;
                }
                let w = w?;
                elem = true;
                im += if t.unit == Unit::H { w * t.v } else { -1.0 / (w * t.v) };
            }
            Unit::Siemens if !admittance => return None,
            Unit::Ohm if admittance => return None,
            _ => {
                if t.j {
                    im += t.v;
                } else {
                    re += t.v;
                }
            }
        }
    }
    Some(((re, im), elem, marked))
}

/// 複素数のインピーダンスとアドミタンスの計算. `w`は角周波数(`@2G`があるとき).
fn network(desc: &str, w: Option<f64>, z0: f64) -> Option<Calc> {
    let d = desc.trim();
    let (d, y_prefix) = match d.get(..2) {
        Some(h) if h.eq_ignore_ascii_case("y ") => (d[2..].trim(), true),
        _ => (d, false),
    };
    let compact: String = d.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace("||", "\u{1}").replace('∥', "\u{1}").replace("//", "\u{1}");
    if compact.is_empty() {
        return None;
    }
    let (form, z, y): (Form, Cx, Cx);
    if compact.contains('\u{1}') {
        // 並列: 各部分は直列の和. アドミタンスを足す
        let mut ysum = (0.0, 0.0);
        let mut n = 0;
        for part in compact.split('\u{1}') {
            let (zi, _, _) = sum_terms(part, w, false)?;
            let yi = cx_inv(zi)?;
            ysum = (ysum.0 + yi.0, ysum.1 + yi.1);
            n += 1;
        }
        if n < 2 {
            return None;
        }
        form = Form::Parallel;
        y = ysum;
        z = cx_inv(y)?;
    } else if y_prefix || split_terms(&compact).iter().any(|t| matches!(parse_term(t), Some(Term { unit: Unit::Siemens, .. }))) {
        form = Form::Y;
        y = sum_terms(&compact, w, true)?.0;
        z = cx_inv(y)?;
    } else {
        let (zz, elem, marked) = sum_terms(&compact, w, false)?;
        // 実部だけの数(`50`)は`@`か単位がないと対象にしない
        if !(marked || w.is_some()) {
            return None;
        }
        form = if elem { Form::Series } else { Form::Z };
        z = zz;
        y = cx_inv(z)?;
    }
    let at = w.map(|w| format!(" at {}", fmt_si(w / (2.0 * PI), "Hz"))).unwrap_or_default();
    let z_row = Some(cx_row(format!("Z = {}", fmt_z(z)), fmt_z(z), "Impedance"));
    let y_row = Some(cx_row(format!("Y = {}", fmt_y(y)), fmt_y(y), "Admittance"));
    let series = w.and_then(|w| series_eq(z, w)).map(|s| cx_row(s.clone(), s, &format!("Series equivalent{at}")));
    let parallel = w.and_then(|w| parallel_eq(y, w)).map(|s| cx_row(s.clone(), s, &format!("Parallel equivalent{at}")));
    let rows: Vec<CalcRow> = match form {
        Form::Z => vec![series, y_row, parallel],
        Form::Y => vec![parallel, z_row, series],
        Form::Series => vec![parallel, z_row, y_row],
        Form::Parallel => vec![series, z_row, y_row],
    }
    .into_iter()
    .flatten()
    .collect();
    // 反射係数Γ, VSWR, リターンロスも添える
    let mut rows = rows;
    if let Some(g) = z_to_gamma(z, z0) {
        rows.extend(gamma_head(g, z0));
        rows.extend(gamma_tail(g, z0));
    }
    (!rows.is_empty()).then_some(Calc { rows })
}

fn cx_row(value: String, copy: String, note: &str) -> CalcRow {
    CalcRow { value, copy, note: note.to_string(), base: None }
}

/// `R ± jX`の文字(接頭辞は大きい方に合わせ, 単位は最後に1つ). 例: `50 + j50 Ω`
fn fmt_z(z: Cx) -> String {
    let m = z.0.abs().max(z.1.abs());
    if m == 0.0 || !m.is_finite() {
        return "0 Ω".into();
    }
    let (f, p) = [(1e12, "T"), (1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p")]
        .into_iter()
        .find(|(f, _)| m >= f * 0.99995)
        .unwrap_or((1e-12, "p"));
    let (re, im) = (z.0 / f, z.1 / f);
    let unit = format!("{p}Ω");
    if tiny(im, m / f) {
        return format!("{} {unit}", sig4(re));
    }
    let j = format!("j{}", sig4(im.abs()));
    if tiny(re, m / f) {
        return format!("{}{j} {unit}", if im < 0.0 { "-" } else { "" });
    }
    format!("{} {} {j} {unit}", sig4(re), if im < 0.0 { "-" } else { "+" })
}

/// アドミタンスの文字. 例: `10 mS - j10 mS`
fn fmt_y(y: Cx) -> String {
    let m = y.0.abs().max(y.1.abs());
    if m == 0.0 || !m.is_finite() {
        return "0 S".into();
    }
    if tiny(y.1, m) {
        return fmt_si(y.0, "S");
    }
    let j = format!("j{}", fmt_si(y.1.abs(), "S"));
    if tiny(y.0, m) {
        return format!("{}{j}", if y.1 < 0.0 { "-" } else { "" });
    }
    format!("{} {} {j}", fmt_si(y.0, "S"), if y.1 < 0.0 { "-" } else { "+" })
}

/// 直列の等価回路(抵抗 + L か C). 例: `50 Ω + 3.979 nH`
fn series_eq(z: Cx, w: f64) -> Option<String> {
    let m = z.0.abs().max(z.1.abs());
    if m == 0.0 {
        return Some("0 Ω".into());
    }
    let mut parts = Vec::new();
    if !tiny(z.0, m) {
        parts.push(fmt_si(z.0, "Ω"));
    }
    if !tiny(z.1, m) {
        parts.push(if z.1 > 0.0 { fmt_si(z.1 / w, "H") } else { fmt_si(-1.0 / (w * z.1), "F") });
    }
    Some(parts.join(" + "))
}

/// 並列の等価回路(抵抗 ∥ L か C). 例: `100 Ω ∥ 7.958 nH`
fn parallel_eq(y: Cx, w: f64) -> Option<String> {
    let m = y.0.abs().max(y.1.abs());
    if m == 0.0 {
        return None;
    }
    let mut parts = Vec::new();
    if !tiny(y.0, m) {
        parts.push(fmt_si(1.0 / y.0, "Ω"));
    }
    if !tiny(y.1, m) {
        parts.push(if y.1 < 0.0 { fmt_si(-1.0 / (w * y.1), "H") } else { fmt_si(y.1 / w, "F") });
    }
    Some(parts.join(" ∥ "))
}

/// 基準インピーダンスの注記. 基準を前提にした行の2行目に付ける.
fn z0_note(base: &str, z0: f64) -> String {
    format!("{base} · Z0 = {}", fmt_si(z0, "Ω"))
}

/// 入力の中の`z0=75`(`z0 75`, `z0=75ohm`, `z0=75Ω`も可)を取り除く. 返すのは(残りの入力, 基準Ω. なければ50).
/// 値が正の実数でないときはErr(50で黙って計算しない).
fn take_z0(t: &str) -> Result<(String, f64), ()> {
    let toks: Vec<&str> = t.split_whitespace().collect();
    let mut rest: Vec<&str> = Vec::new();
    let mut z0 = None;
    let mut i = 0;
    while i < toks.len() {
        let tok = toks[i];
        i += 1;
        let l = tok.to_ascii_lowercase();
        let after = l.strip_prefix("z0").filter(|a| a.is_empty() || a.starts_with('='));
        if z0.is_some() || after.is_none() {
            rest.push(tok);
            continue;
        }
        let mut val = tok[2..].trim_start_matches('=');
        while val.is_empty() {
            val = toks.get(i).copied().ok_or(())?.trim_start_matches('=');
            i += 1;
        }
        let term = parse_term(val).ok_or(())?;
        if term.j || !matches!(term.unit, Unit::None | Unit::Ohm) || !(term.v > 0.0) || !term.v.is_finite() {
            return Err(());
        }
        z0 = Some(term.v);
    }
    Ok((rest.join(" "), z0.unwrap_or(50.0)))
}

/// 複素数の割り算. 分母が0に近ければNone.
fn cx_div(a: Cx, b: Cx) -> Option<Cx> {
    let d = b.0 * b.0 + b.1 * b.1;
    if !(d > 1e-12) || !d.is_finite() {
        return None;
    }
    Some(((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d))
}

/// Γ = (Z - Z0)/(Z + Z0)
fn z_to_gamma(z: Cx, z0: f64) -> Option<Cx> {
    cx_div((z.0 - z0, z.1), (z.0 + z0, z.1))
}

/// Z = Z0(1 + Γ)/(1 - Γ). Γ=1(開放)はNone.
fn gamma_to_z(g: Cx, z0: f64) -> Option<Cx> {
    let r = cx_div((1.0 + g.0, g.1), (1.0 - g.0, -g.1))?;
    Some((r.0 * z0, r.1 * z0))
}

/// Y = (1 - Γ)/(Z0(1 + Γ)). Γ=-1(短絡)はNone.
fn gamma_to_y(g: Cx, z0: f64) -> Option<Cx> {
    let r = cx_div((1.0 - g.0, -g.1), (1.0 + g.0, g.1))?;
    Some((r.0 / z0, r.1 / z0))
}

/// `a + jb`の文字(有効数字4桁)
fn fmt_rect(a: Cx) -> String {
    let m = a.0.abs().max(a.1.abs());
    if m == 0.0 {
        return "0".into();
    }
    if tiny(a.1, m) {
        return sig4(a.0);
    }
    let j = format!("j{}", sig4(a.1.abs()));
    if tiny(a.0, m) {
        return format!("{}{j}", if a.1 < 0.0 { "-" } else { "" });
    }
    format!("{} {} {j}", sig4(a.0), if a.1 < 0.0 { "-" } else { "+" })
}

/// Γの行(直交と極形式)
fn gamma_head(g: Cx, z0: f64) -> Vec<CalcRow> {
    let m = g.0.hypot(g.1);
    let deg = if m == 0.0 { 0.0 } else { g.1.atan2(g.0).to_degrees() };
    let polar = format!("{} ∠ {}°", sig4(m), sig4(deg));
    vec![
        cx_row(format!("Γ = {}", fmt_rect(g)), fmt_rect(g), &z0_note("Reflection coefficient (rectangular)", z0)),
        cx_row(polar.clone(), polar, &z0_note("Reflection coefficient (magnitude ∠ angle)", z0)),
    ]
}

/// VSWRとリターンロスの行. |Γ|>1のときは出さない.
fn gamma_tail(g: Cx, z0: f64) -> Vec<CalcRow> {
    let m = g.0.hypot(g.1);
    let mut rows = Vec::new();
    if !m.is_finite() || m > 1.0 + 1e-9 {
        return rows;
    }
    let vswr = if m >= 1.0 - 1e-9 { "∞".to_string() } else { sig4((1.0 + m) / (1.0 - m)) };
    rows.push(row(vswr, &z0_note("VSWR", z0)));
    let rl = if m < 1e-12 { "∞ dB".to_string() } else { format!("{} dB", fixed2((-20.0 * m.log10()).max(0.0) + 0.0)) };
    rows.push(row(rl, &z0_note("Return loss", z0)));
    rows
}

/// ΓからZ, Y(開放や短絡は`open`)
fn gamma_zy_rows(g: Cx, z0: f64) -> Vec<CalcRow> {
    let z = gamma_to_z(g, z0);
    let y = gamma_to_y(g, z0);
    let (zn, yn) = (z0_note("Impedance", z0), z0_note("Admittance", z0));
    let zr = match z {
        Some(z) => cx_row(format!("Z = {}", fmt_z(z)), fmt_z(z), &zn),
        None => cx_row("Z = open".into(), "open".into(), &zn),
    };
    let yr = match y {
        Some(y) => cx_row(format!("Y = {}", fmt_y(y)), fmt_y(y), &yn),
        None => cx_row("Y = open".into(), "open".into(), &yn),
    };
    vec![zr, yr]
}

/// `gamma 0.2+j0.4`, `gamma 0.2 0.4`, `gamma 0.4472<63.43`(`∠`も可. `@2G`で等価回路も). 値が1つだけのときは読まない.
fn gamma_input(t: &str, z0: f64) -> Option<Calc> {
    let (body, w) = match t.split_once('@') {
        Some((l, r)) => (l.trim(), Some(2.0 * PI * freq(r)?)),
        None => (t.trim(), None),
    };
    let mut rest = None;
    for k in ["gamma", "|γ|", "|Γ|", "γ", "Γ", "g"] {
        if body.len() > k.len() && body.is_char_boundary(k.len()) && body[..k.len()].eq_ignore_ascii_case(k) {
            rest = Some(body[k.len()..].trim_start_matches([' ', '=', ':']).trim());
            break;
        }
    }
    let rest = rest.filter(|r| !r.is_empty())?;
    let g: Cx = if let Some(i) = rest.find(['<', '∠']) {
        let m: f64 = rest[..i].trim().parse().ok()?;
        let mut a = rest[i..].trim_start_matches(['<', '∠']).trim();
        loop {
            let n = a.len();
            a = a.trim_end_matches(['°', ' ']);
            if let Some(r) = a.strip_suffix("deg") {
                a = r;
            }
            if a.len() == n {
                break;
            }
        }
        let th = a.trim().parse::<f64>().ok()?.to_radians();
        (m * th.cos(), m * th.sin())
    } else if rest.contains(['j', 'J']) {
        let compact: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
        let (mut re, mut im) = (0.0, 0.0);
        for t in split_terms(&compact) {
            let t = parse_term(t)?;
            if t.unit != Unit::None {
                return None;
            }
            if t.j {
                im += t.v;
            } else {
                re += t.v;
            }
        }
        (re, im)
    } else {
        let v: Vec<&str> = rest.split([',', ' ']).filter(|p| !p.is_empty()).collect();
        if v.len() != 2 {
            return None;
        }
        (v[0].parse().ok()?, v[1].parse().ok()?)
    };
    if !g.0.is_finite() || !g.1.is_finite() {
        return None;
    }
    let mut rows = gamma_head(g, z0);
    rows.extend(gamma_zy_rows(g, z0));
    if let Some(w) = w {
        let at = format!(" at {}", fmt_si(w / (2.0 * PI), "Hz"));
        if let Some(s) = gamma_to_z(g, z0).and_then(|z| series_eq(z, w)) {
            rows.push(cx_row(s.clone(), s, &format!("Series equivalent{at}")));
        }
        if let Some(s) = gamma_to_y(g, z0).and_then(|y| parallel_eq(y, w)) {
            rows.push(cx_row(s.clone(), s, &format!("Parallel equivalent{at}")));
        }
    }
    rows.extend(gamma_tail(g, z0));
    Some(Calc { rows })
}

/// RFの計算. 合わなければNone.
fn rf(s: &str, z0: f64) -> Option<Calc> {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let mut rows = Vec::new();
    // 複素数の反射係数: gamma 0.2+j0.4, gamma 0.2 0.4, gamma 0.4472<63.43(値が1つだけのときは下の大きさの扱い)
    if let Some(c) = gamma_input(t, z0) {
        return Some(c);
    }
    if lower.contains(" il") {
        return through_loss(&lower);
    }

    // リアクタンス: 10nH@2.4GHz, 1pF@2.4GHz
    if let Some((left, right)) = t.split_once('@') {
        // 10nH@2G のようにHzは省いてよい(2026-11-24)
        let f = freq(right)?;
        if let Some(l) = quantity(left, "H") {
            rows.push(row(fmt_si(2.0 * PI * f * l, "Ω"), "XL = 2πfL"));
        } else if let Some(c) = quantity(left, "F") {
            rows.push(row(fmt_si(1.0 / (2.0 * PI * f * c), "Ω"), "XC = 1/(2πfC)"));
        } else {
            // `50+j50@2G`, `50+3.979nH@2G`, `100||7.958nH@2G`など
            return network(left, Some(2.0 * PI * f), z0);
        }
        return Some(Calc { rows });
    }
    // 共振: 10nH 1pF → 周波数, 10nH 2.4G → C, 1pF 2.4G → L
    if let Some(c) = resonance(t) {
        return Some(c);
    }
    // 複素数のインピーダンス・アドミタンス: 50+j50, 10mS-j10mS
    if let Some(c) = network(t, None, z0) {
        return Some(c);
    }
    if let Some(v) = quantity(t, "dBm") {
        let w = 10f64.powf(v / 10.0) / 1000.0;
        rows.push(row(fmt_si(w, "W"), "Power"));
        rows.push(voltage_row(w, z0));
        return Some(Calc { rows });
    }
    if lower.ends_with("dbw") {
        let v = quantity(t, "dBW")?;
        rows.push(row(fmt_si(10f64.powf(v / 10.0), "W"), "Power"));
        rows.push(row(format!("{} dBm", fixed2(v + 30.0)), "dBm"));
        return Some(Calc { rows });
    }
    if let Some(v) = quantity(t, "dB") {
        rows.push(row(format!("x{}", sig4(10f64.powf(v / 10.0))), "Power ratio"));
        rows.push(row(format!("x{}", sig4(10f64.powf(v / 20.0))), "Voltage ratio"));
        return Some(Calc { rows });
    }
    if !lower.ends_with("dbm") && !lower.ends_with("db") {
        if let Some(w) = quantity(t, "W") {
            if w > 0.0 {
                rows.push(row(format!("{} dBm", fixed2(10.0 * (w * 1000.0).log10())), "dBm"));
                return Some(Calc { rows });
            }
        }
    }
    // 倍率→dB: x2, 2x
    let ratio = lower.strip_prefix('x').or_else(|| lower.strip_suffix('x')).and_then(plain_number);
    if let Some(r) = ratio.filter(|r| *r > 0.0) {
        rows.push(row(format!("{} dB", fixed2(10.0 * r.log10())), "As power ratio"));
        rows.push(row(format!("{} dB", fixed2(20.0 * r.log10())), "As voltage ratio"));
        return Some(Calc { rows });
    }
    // VSWR, リターンロス, |Γ|
    let after = |keys: &[&str]| -> Option<f64> {
        for k in keys {
            if let Some(rest) = lower.strip_prefix(k) {
                let rest = rest.trim_start_matches([' ', '=', ':']);
                let rest = rest.trim_end_matches("db").trim();
                if let Some(v) = plain_number(rest) {
                    return Some(v);
                }
            }
        }
        None
    };
    if let Some(v) = after(&["vswr"]) {
        if v >= 1.0 {
            reflection_rows((v - 1.0) / (v + 1.0), &mut rows, "vswr");
            return Some(Calc { rows });
        }
    }
    if let Some(v) = after(&["rl", "return loss"]) {
        let rl = v.abs();
        reflection_rows(10f64.powf(-rl / 20.0), &mut rows, "rl");
        return Some(Calc { rows });
    }
    if let Some(v) = after(&["gamma", "γ", "|γ|", "g"]) {
        if (0.0..1.0).contains(&v) {
            reflection_rows(v, &mut rows, "gamma");
            return Some(Calc { rows });
        }
    }
    None
}

/// 四則演算の式を読む再帰下降の構文解析. `+ - * / ^`, 括弧, 小数, 単項の`+`と`-`, 後置の`%`.
/// `^`は右結合で単項マイナスより強い(`-2^2`は-4). `%`は100で割るだけ.
struct Arith<'a> {
    s: &'a [u8],
    i: usize,
}

impl Arith<'_> {
    fn peek(&mut self) -> Option<u8> {
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
        self.s.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(c @ (b'+' | b'-')) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if c == b'+' { v + r } else { v - r };
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        while let Some(c @ (b'*' | b'/')) = self.peek() {
            self.i += 1;
            let r = self.unary()?;
            v = if c == b'*' { v * r } else { v / r };
        }
        Some(v)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            b'-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            b'+' => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.peek() == Some(b'^') {
            self.i += 1;
            let e = self.unary()?;
            return Some(base.powf(e));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut v = self.primary()?;
        while self.peek() == Some(b'%') {
            self.i += 1;
            v /= 100.0;
        }
        Some(v)
    }

    fn primary(&mut self) -> Option<f64> {
        if self.peek()? == b'(' {
            self.i += 1;
            let v = self.expr()?;
            if self.peek()? != b')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
    }
}

/// 計算結果の表示. 整数はそのまま, 小数は有効数字10桁で末尾の0を消す.
fn fmt_num(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if v.abs() >= 1e21 {
        return format!("{v:e}");
    }
    if v == v.trunc() {
        return format!("{v:.0}");
    }
    let digits = (9 - v.abs().log10().floor() as i32).clamp(0, 40) as usize;
    let s = format!("{v:.digits$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// ふつうの計算. 演算子か括弧を含み, 数字・演算子・括弧・空白・小数点だけでできているときだけ答える.
/// 数字だけ(`1187`)や先頭のマイナスだけ(`-5`)は計算の形にしない. 解けない式や0での割り算はNone.
fn arith(s: &str) -> Option<Calc> {
    if !s.chars().all(|c| c.is_ascii_digit() || c.is_ascii_whitespace() || "+-*/^()%.".contains(c)) {
        return None;
    }
    let body = s.trim();
    let has_op = body.char_indices().any(|(i, c)| "+*/^()%".contains(c) || (c == '-' && i > 0));
    if !has_op {
        return None;
    }
    let mut p = Arith { s: body.as_bytes(), i: 0 };
    let v = p.expr()?;
    if p.peek().is_some() || !v.is_finite() {
        return None;
    }
    Some(Calc { rows: vec![row(fmt_num(v), "Result")] })
}

/// 入力が計算の形なら結果を返す.
pub fn evaluate(input: &str) -> Option<Calc> {
    let t = input.trim();
    if t.is_empty() || t.len() > 128 {
        return None;
    }
    // `3dB`が16進数(0x3DB)にならないよう, 単位の付いたRFの計算を先に見る
    // ふつうの計算はそのあとに見る
    // 基準インピーダンス(`z0=75`)を先に取り除く. 値が読めないときは答えを出さない
    let (t, z0) = take_z0(t).ok()?;
    if t.is_empty() {
        return None;
    }
    rf(&t, z0).or_else(|| radix(&t)).or_else(|| arith(&t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals(s: &str) -> Vec<String> {
        evaluate(s).unwrap().rows.into_iter().map(|r| r.value).collect()
    }

    #[test]
    fn radix_conversion() {
        // 打った進数以外の2つだけ. 16進数は0x, 2進数は0b, 10進数は数字だけ
        let c = evaluate("0x1F").unwrap();
        assert_eq!(vals("0x1F"), vec!["31", "0001 1111"]);
        assert_eq!(c.rows[1].copy, "0b00011111");
        assert_eq!(vals("0b111011"), vec!["0x3B", "59"]);
        assert_eq!(vals("1187"), vec!["0x4A3", "0100 1010 0011"]);
        assert_eq!(vals("1110"), vec!["0x456", "0100 0101 0110"]);
        assert_eq!(vals("2_024"), vec!["0x7E8", "0111 1110 1000"]);
        // 印のない16進数は読まない(アプリ名の打ちかけと区別できないため)
        assert!(evaluate("FF").is_none());
        assert!(evaluate("1Fh").is_none());
        assert!(evaluate("12ab").is_none());
        assert!(evaluate("each").is_none());
    }

    #[test]
    fn reactance_and_resonance() {
        assert_eq!(vals("10nH@2.4GHz"), vec!["150.8 Ω"]);
        assert_eq!(vals("1pF@2.4GHz"), vec!["66.31 Ω"]);
        // Hzは省ける
        assert_eq!(vals("10nH@2.4G"), vec!["150.8 Ω"]);
        assert_eq!(vals("10nH@2G"), vals("10nH@2GHz"));
        assert_eq!(vals("1pF@100M"), vals("1pF@100MHz"));
        assert_eq!(vals("10nH 1pF"), vec!["1.592 GHz"]);
        assert_eq!(vals("1pF, 10nH"), vec!["1.592 GHz"]);
    }

    #[test]
    fn power() {
        // dBm: Wと, 50Ωにかけたときの電圧の振幅(ピーク)
        assert_eq!(vals("10dBm"), vec!["10 mW", "1 V"]);
        let c = evaluate("10dBm").unwrap();
        assert!(c.rows[1].note.contains("Vrms 707.1 mV"), "{}", c.rows[1].note);
        assert!(c.rows[1].note.contains("Vpp 2 V"), "{}", c.rows[1].note);
        assert_eq!(vals("-30 dBm"), vec!["1 µW", "10 mV"]);
        assert_eq!(vals("1W"), vec!["30.00 dBm"]);
        assert_eq!(vals("100mW"), vec!["20.00 dBm"]);
        assert_eq!(vals("3dB"), vec!["x1.995", "x1.413"]);
        assert_eq!(vals("x2"), vec!["3.01 dB", "6.02 dB"]);
    }

    #[test]
    fn reflection() {
        let v = vals("vswr 1.5");
        assert_eq!(v[0], "13.98 dB");
        assert_eq!(v[1], "0.200");
        assert_eq!(v[2], "4.00 %");
        let v = vals("rl 20");
        assert_eq!(v[0], "1.222");
        let v = vals("gamma 0.2");
        assert_eq!(v[0], "1.5");
    }

    #[test]
    fn reflection_through_loss() {
        // 出力でVSWR 10, 間に2dBの損失: 入力から見たリターンロスは1.74+4dB
        let v = vals("vswr 10 IL 2dB");
        assert_eq!(v, vec!["3.134", "5.74 dB", "0.516"]);
        assert_eq!(vals("vswr 10 il 2"), v);
        assert_eq!(vals("VSWR 10 IL 2 dB"), v);
        // リターンロスからでも
        assert_eq!(vals("rl 10 il 1")[1], "12.00 dB");
        let c = evaluate("vswr 10 il 2dB").unwrap();
        assert!(c.rows[0].note.contains("output VSWR 10 (RL 1.74 dB)"), "{}", c.rows[0].note);
        assert!(evaluate("vswr 0.5 il 2").is_none());
    }

    #[test]
    fn arithmetic() {
        assert_eq!(vals("(12+3)*4"), vec!["60"]);
        assert_eq!(vals("2^10"), vec!["1024"]);
        assert_eq!(vals("1500*1.1"), vec!["1650"]);
        assert_eq!(vals("200*15%"), vec!["30"]);
        assert_eq!(vals("15%+1"), vec!["1.15"]);
        assert_eq!(vals("1/3"), vec!["0.3333333333"]);
        assert_eq!(vals("-(2+3)"), vec!["-5"]);
        assert_eq!(vals("-2^2"), vec!["-4"]);
        assert_eq!(vals("2 ^ 3 ^ 2"), vec!["512"]);
        assert_eq!(vals("0.1+0.2"), vec!["0.3"]);
        let c = evaluate("1500*1.1").unwrap();
        assert_eq!(c.rows[0].copy, c.rows[0].value);
    }

    #[test]
    fn arithmetic_rejects() {
        assert!(evaluate("1/0").is_none());
        assert!(evaluate("(1+2").is_none());
        assert!(evaluate("1+").is_none());
        assert!(evaluate("-5").is_none());
        assert!(evaluate("1.5").is_none());
        assert!(evaluate("0x1+1").is_none());
        assert!(evaluate("3dB+1").is_none());
        // 進数と文字はそのまま
        assert_eq!(vals("1187"), vec!["0x4A3", "0100 1010 0011"]);
        assert!(evaluate("each").is_none());
    }

   /// Γ, VSWR, リターンロスの行を除いた値(等価回路やZ, Yの確かめ用)
    fn core(s: &str) -> Vec<String> {
        evaluate(s)
            .unwrap()
            .rows
            .into_iter()
            .filter(|r| !(r.note.starts_with("Reflection coefficient") || r.note.starts_with("VSWR") || r.note.starts_with("Return loss")))
            .map(|r| r.value)
            .collect()
    }

    fn close(a: Cx, b: Cx) -> bool {
        (a.0 - b.0).abs() < 1e-9 * (1.0 + b.0.abs()) && (a.1 - b.1).abs() < 1e-9 * (1.0 + b.1.abs())
    }

    #[test]
    fn gamma_from_z() {
        // 50+j50 ↔ Γ=0.2+j0.4 ↔ 0.4472∠63.43° ↔ Y=10 mS - j10 mS
        assert_eq!(vals("50+j50"), vec!["Y = 10 mS - j10 mS", "Γ = 0.2 + j0.4", "0.4472 ∠ 63.43°", "2.618", "6.99 dB"]);
        assert_eq!(vals("gamma 0.2+j0.4"), vec!["Γ = 0.2 + j0.4", "0.4472 ∠ 63.43°", "Z = 50 + j50 Ω", "Y = 10 mS - j10 mS", "2.618", "6.99 dB"]);
        for s in ["gamma 0.2 0.4", "Γ 0.2,0.4", "g 0.2, 0.4", "gamma 0.2 + j 0.4", "gamma = 0.4472<63.435", "gamma 0.4472∠63.435°", "γ 0.4472 ∠ 63.435 deg"] {
            let v = vals(s);
            assert_eq!(v[2], "Z = 50 + j50 Ω", "{s}: {v:?}");
            assert_eq!(v[3], "Y = 10 mS - j10 mS", "{s}: {v:?}");
        }
        // 等価回路も
        assert_eq!(vals("gamma 0.2+j0.4@2G"), vec!["Γ = 0.2 + j0.4", "0.4472 ∠ 63.43°", "Z = 50 + j50 Ω", "Y = 10 mS - j10 mS", "50 Ω + 3.979 nH", "100 Ω ∥ 7.958 nH", "2.618", "6.99 dB"]);
        // 値が1つだけは今までどおり大きさだけ
        assert_eq!(vals("gamma 0.2")[0], "1.5");
        // Z=50はΓ=0
        let v = vals("50+j0@2G");
        assert_eq!(v[v.len() - 4..], ["Γ = 0".to_string(), "0 ∠ 0°".to_string(), "1".to_string(), "∞ dB".to_string()]);
        // 負の角度
        assert_eq!(vals("gamma 0.4472<-63.435")[0], "Γ = 0.2 - j0.4");
    }

    #[test]
    fn gamma_round_trips() {
        for z in [(25.0, -25.0), (100.0, -50.0), (50.0, 50.0), (10.0, 0.0), (0.0, 30.0), (200.0, 300.0)] {
            let g = z_to_gamma(z, 50.0).unwrap();
            assert!(close(gamma_to_z(g, 50.0).unwrap(), z), "{z:?}");
            let y = cx_inv(z).unwrap();
            let g2 = z_to_gamma(cx_inv(y).unwrap(), 50.0).unwrap();
            assert!(close(gamma_to_y(g2, 50.0).unwrap(), y), "{z:?}");
        }
        assert_eq!(z_to_gamma((50.0, 0.0), 50.0), Some((0.0, 0.0)));
        // 文字からも戻る(25-j25 → Γ → Z)
        let v = vals("25-j25");
        let g = v.iter().find(|x| x.starts_with("Γ = ")).unwrap().trim_start_matches("Γ = ").to_string();
        let back = vals(&format!("gamma {g}"));
        assert_eq!(back[2], "Z = 25 - j25 Ω", "{g}: {back:?}");
    }

    #[test]
    fn gamma_edges() {
        // 開放と短絡
        assert_eq!(vals("gamma 1 0"), vec!["Γ = 1", "1 ∠ 0°", "Z = open", "Y = 0 S", "∞", "0.00 dB"]);
        let v = vals("gamma -1 0");
        assert_eq!(v[2], "Z = 0 Ω");
        assert_eq!(v[3], "Y = open");
        assert_eq!(v[4], "∞");
        // |Γ|>1: Z, Yは出し, VSWRとリターンロスは出さない
        let v = vals("gamma 2 0");
        assert_eq!(v, vec!["Γ = 2", "2 ∠ 0°", "Z = -150 Ω", "Y = -6.667 mS"]);
        // 等価回路は開放や短絡では出さない
        assert!(!vals("gamma 1 0@2G").iter().any(|x| x.contains("nH") || x.contains("pF")));
        assert!(evaluate("gamma abc").is_none());
    }

    #[test]
    fn resonance_solves_any_one() {
        assert_eq!(core("10nH 2.4G"), vec!["0.4398 pF"]);
        assert_eq!(core("2.4GHz 10nH"), vec!["0.4398 pF"]);
        assert_eq!(core("1pF 2.4G"), vec!["4.398 nH"]);
        assert_eq!(core("2.4G 1pF"), vec!["4.398 nH"]);
        // 今までどおり
        assert_eq!(core("10nH 1pF"), vec!["1.592 GHz"]);
        assert_eq!(core("10nH@2.4GHz"), vec!["150.8 Ω"]);
    }

    #[test]
    fn impedance_to_admittance() {
        assert_eq!(core("50+j50"), vec!["Y = 10 mS - j10 mS"]);
        for s in ["50 + j 50", "50+50j", "50+j50 ohm", "50+j50Ω"] {
            assert_eq!(core(s), vec!["Y = 10 mS - j10 mS"], "{s}");
        }
        assert_eq!(core("50-j50"), vec!["Y = 10 mS + j10 mS"]);
        assert_eq!(core("j50"), vec!["Y = -j20 mS"]);
        assert_eq!(core("-j50"), vec!["Y = j20 mS"]);
        // 四則演算と実部だけの数は今のまま
        assert_eq!(core("50+50"), vec!["100"]);
        assert_eq!(core("50"), vec!["0x32", "0011 0010"]);
    }

    #[test]
    fn impedance_with_frequency() {
        assert_eq!(core("50+j50@2G"), vec!["50 Ω + 3.979 nH", "Y = 10 mS - j10 mS", "100 Ω ∥ 7.958 nH"]);
        assert_eq!(core("50-j50@2G"), vec!["50 Ω + 1.592 pF", "Y = 10 mS + j10 mS", "100 Ω ∥ 0.7958 pF"]);
        // 抵抗だけ, 素子だけ
        assert_eq!(core("50@2G"), vec!["50 Ω", "Y = 20 mS", "50 Ω"]);
        assert_eq!(core("j50@2G"), vec!["3.979 nH", "Y = -j20 mS", "3.979 nH"]);
        // 0除算は行を出さない
        assert!(evaluate("0@2G").is_none());
    }

    #[test]
    fn admittance_input() {
        for s in ["10m-j10mS", "10mS-j10mS", "y 0.01-j0.01", "Y 0.01 - j0.01"] {
            assert_eq!(core(s), vec!["Z = 50 + j50 Ω"], "{s}");
        }
        assert_eq!(core("10mS-j10mS@2G"), vec!["100 Ω ∥ 7.958 nH", "Z = 50 + j50 Ω", "50 Ω + 3.979 nH"]);
    }

    #[test]
    fn series_parallel_conversion() {
        for s in ["50+3.979nH@2G", "50ohm + 3.979nH @2G", "50Ω+3.979nH@2G"] {
            assert_eq!(core(s)[0], "100 Ω ∥ 7.958 nH", "{s}");
        }
        let v = core("50+3.979nH@2G");
        assert_eq!(v[1], "Z = 50 + j50 Ω");
        assert_eq!(v[2], "Y = 10 mS - j10 mS");
        for s in ["100||7.958nH@2G", "100∥7.958nH@2G", "100//7.958nH@2G", "100 Ω ∥ 7.958 nH @2G"] {
            assert_eq!(core(s)[0], "50 Ω + 3.979 nH", "{s}");
        }
        // Cも同じ
        assert_eq!(core("50+1.5915pF@2G")[0], "100 Ω ∥ 0.7958 pF");
        assert_eq!(core("100||0.7958pF@2G")[0], "50 Ω + 1.592 pF");
    }

    #[test]
    fn round_trips() {
        // Z → Y → Z
        let y = core("50+j50")[0].trim_start_matches("Y = ").to_string();
        assert_eq!(core(&y), vec!["Z = 50 + j50 Ω"]);
        let y = core("12.5-j33")[0].trim_start_matches("Y = ").to_string();
        let z = core(&y)[0].clone();
        assert!(z.starts_with("Z = 12.5 - j33"), "{z}");
        // 直列 → 並列 → 直列
        let p = core("50+3.979nH@2G")[0].clone();
        assert_eq!(core(&format!("{p}@2G"))[0], "50 Ω + 3.979 nH");
        let s = core("100||7.958nH@2G")[0].clone();
        assert_eq!(core(&format!("{s}@2G"))[0], "100 Ω ∥ 7.958 nH");
    }

    fn notes(s: &str) -> Vec<String> {
        evaluate(s).unwrap().rows.into_iter().map(|r| r.note).collect()
    }

    #[test]
    fn reference_impedance() {
        // 既定は50 Ω. 基準を前提にした行にだけ付く
        assert!(notes("gamma 0.2+j0.4").iter().all(|n| n.contains("Z0 = 50 Ω")), "{:?}", notes("gamma 0.2+j0.4"));
        assert!(notes("10dBm")[1].contains("Z0 = 50 Ω"));
        let n = notes("50+j50@2G");
        assert!(!n[0].contains("Z0") && !n[1].contains("Z0") && !n[2].contains("Z0"), "{n:?}");
        assert!(n[3..].iter().all(|x| x.contains("Z0 = 50 Ω")), "{n:?}");
        for s in ["50+j50", "10nH 2.4G", "10nH@2.4G", "vswr 1.5", "10mS-j10mS@2G"] {
            let n = notes(s);
            let free = if s == "50+j50" { &n[..1] } else { &n[..] };
            let free = if s == "10mS-j10mS@2G" { &n[..3] } else { free };
            assert!(free.iter().all(|x| !x.contains("Z0")), "{s}: {n:?}");
        }
        // z0=75
        for s in ["gamma 0.2+j0.4 z0=75", "z0=75 gamma 0.2+j0.4", "gamma 0.2+j0.4 Z0 75", "gamma 0.2+j0.4 z0=75ohm", "gamma 0.2+j0.4 z0 = 75Ω"] {
            let v = vals(s);
            assert_eq!(v[2], "Z = 75 + j75 Ω", "{s}");
            assert!(notes(s)[2].contains("Z0 = 75 Ω"), "{s}");
        }
        // Z=75, z0=75 → Γ=0. 50+j50, z0=100 → Γ=-0.2+j0.4(手計算: (-50+j50)/(150+j50))
        assert_eq!(vals("75@2G z0=75")[3], "Γ = 0");
        assert_eq!(vals("75+j0 z0=75")[1], "Γ = 0");
        assert_eq!(vals("50+j50 z0=100")[1], "Γ = -0.2 + j0.4");
        // 電圧: √(0.01×75)=0.8660 V(実効値)
        let n = notes("10dBm z0=75");
        assert!(n[1].contains("Z0 = 75 Ω") && n[1].contains("Vrms 866 mV"), "{n:?}");
        // 基準によらない計算は同じ答え
        assert_eq!(vals("50+j50 z0=75")[0], "Y = 10 mS - j10 mS");
        assert_eq!(vals("(12+3)*4 z0=75"), vec!["60"]);
        // 読めない値や0, 負は答えを出さない
        for s in ["gamma 0.2+j0.4 z0=0", "gamma 0.2+j0.4 z0=-50", "gamma 0.2+j0.4 z0=abc", "gamma 0.2+j0.4 z0=", "gamma 0.2+j0.4 z0=j50", "z0=75"] {
            assert!(evaluate(s).is_none(), "{s}");
        }
    }

    #[test]
    fn not_calc() {
        assert!(evaluate("pwpt").is_none());
        assert!(evaluate("report 2024").is_none());
        assert!(evaluate("dB").is_none());
    }
}

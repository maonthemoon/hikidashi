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

/// 計算の結果. `top`がtrueなら検索結果より上に出す(K6: 印付きの数やRFの計算).
#[derive(Debug, Clone, PartialEq)]
pub struct Calc {
    pub rows: Vec<CalcRow>,
    pub top: bool,
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
    format!("{:.3e} {}", v, unit)
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
/// - `0x`, `0b`の付いたもの: 変換を検索結果より上に出す.
/// - 数字だけ: 10進数として, 16進数と2進数を検索結果の下に出す.
fn radix(s: &str) -> Option<Calc> {
    let raw = s.trim();
    if raw.contains(char::is_whitespace) {
        return None;
    }
    let t: String = raw.chars().filter(|c| *c != '_').collect();
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        let v = u128::from_str_radix(h, 16).ok()?;
        return Some(Calc { rows: vec![dec_row(v, "dec"), bin_row(v, "bin")], top: true });
    }
    if let Some(b) = lower.strip_prefix("0b") {
        let v = u128::from_str_radix(b, 2).ok()?;
        return Some(Calc { rows: vec![hex_row(v, "hex"), dec_row(v, "dec")], top: true });
    }
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        let d = t.parse::<u128>().ok()?;
        return Some(Calc { rows: vec![hex_row(d, "hex"), bin_row(d, "bin")], top: false });
    }
    None
}

/// L・Cの組み合わせを読む. 区切りは空白, `,`, `&`.
fn l_and_c(s: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = s.split([' ', ',', '&']).filter(|p| !p.is_empty()).collect();
    if parts.len() != 2 {
        return None;
    }
    let (a, b) = (parts[0], parts[1]);
    if let (Some(l), Some(c)) = (quantity(a, "H"), quantity(b, "F")) {
        return Some((l, c));
    }
    if let (Some(c), Some(l)) = (quantity(a, "F"), quantity(b, "H")) {
        return Some((l, c));
    }
    None
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

/// 50Ωにその電力をかけたときの電圧の振幅(ピーク). 2行目に実効値とピークtoピーク
fn voltage_row(w: f64) -> CalcRow {
    let vp = (2.0 * w * 50.0).sqrt();
    let note = format!("Peak voltage at 50 Ω · Vrms {} · Vpp {}", fmt_si(vp / 2f64.sqrt(), "V"), fmt_si(vp * 2.0, "V"));
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
    Some(Calc { rows, top: true })
}

/// RFの計算. 合わなければNone.
fn rf(s: &str) -> Option<Calc> {
    let t = s.trim();
    let lower = t.to_ascii_lowercase();
    let mut rows = Vec::new();
    if lower.contains(" il") {
        return through_loss(&lower);
    }

    // リアクタンス: 10nH@2.4GHz, 1pF@2.4GHz
    if let Some((left, right)) = t.split_once('@') {
        // 10nH@2G のようにHzは省いてよい(2026-11-24)
        let f = quantity(right, "Hz").or_else(|| quantity(&format!("{}Hz", right.trim()), "Hz"))?;
        if let Some(l) = quantity(left, "H") {
            rows.push(row(fmt_si(2.0 * PI * f * l, "Ω"), "XL = 2πfL"));
        } else if let Some(c) = quantity(left, "F") {
            rows.push(row(fmt_si(1.0 / (2.0 * PI * f * c), "Ω"), "XC = 1/(2πfC)"));
        } else {
            return None;
        }
        return Some(Calc { rows, top: true });
    }
    // 共振周波数: 10nH 1pF
    if let Some((l, c)) = l_and_c(t) {
        rows.push(row(fmt_si(1.0 / (2.0 * PI * (l * c).sqrt()), "Hz"), "Resonant frequency f0 = 1/(2π√LC)"));
        return Some(Calc { rows, top: true });
    }
    if let Some(v) = quantity(t, "dBm") {
        let w = 10f64.powf(v / 10.0) / 1000.0;
        rows.push(row(fmt_si(w, "W"), "Power"));
        rows.push(voltage_row(w));
        return Some(Calc { rows, top: true });
    }
    if lower.ends_with("dbw") {
        let v = quantity(t, "dBW")?;
        rows.push(row(fmt_si(10f64.powf(v / 10.0), "W"), "Power"));
        rows.push(row(format!("{} dBm", fixed2(v + 30.0)), "dBm"));
        return Some(Calc { rows, top: true });
    }
    if let Some(v) = quantity(t, "dB") {
        rows.push(row(format!("x{}", sig4(10f64.powf(v / 10.0))), "Power ratio"));
        rows.push(row(format!("x{}", sig4(10f64.powf(v / 20.0))), "Voltage ratio"));
        return Some(Calc { rows, top: true });
    }
    if !lower.ends_with("dbm") && !lower.ends_with("db") {
        if let Some(w) = quantity(t, "W") {
            if w > 0.0 {
                rows.push(row(format!("{} dBm", fixed2(10.0 * (w * 1000.0).log10())), "dBm"));
                return Some(Calc { rows, top: true });
            }
        }
    }
    // 倍率→dB: x2, 2x
    let ratio = lower.strip_prefix('x').or_else(|| lower.strip_suffix('x')).and_then(plain_number);
    if let Some(r) = ratio.filter(|r| *r > 0.0) {
        rows.push(row(format!("{} dB", fixed2(10.0 * r.log10())), "As power ratio"));
        rows.push(row(format!("{} dB", fixed2(20.0 * r.log10())), "As voltage ratio"));
        return Some(Calc { rows, top: true });
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
            return Some(Calc { rows, top: true });
        }
    }
    if let Some(v) = after(&["rl", "return loss"]) {
        let rl = v.abs();
        reflection_rows(10f64.powf(-rl / 20.0), &mut rows, "rl");
        return Some(Calc { rows, top: true });
    }
    if let Some(v) = after(&["gamma", "γ", "|γ|", "g"]) {
        if (0.0..1.0).contains(&v) {
            reflection_rows(v, &mut rows, "gamma");
            return Some(Calc { rows, top: true });
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
    Some(Calc { rows: vec![row(fmt_num(v), "Result")], top: false })
}

/// 入力が計算の形なら結果を返す.
pub fn evaluate(input: &str) -> Option<Calc> {
    let t = input.trim();
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    // `3dB`が16進数(0x3DB)にならないよう, 単位の付いたRFの計算を先に見る
    // ふつうの計算はそのあとに見る
    rf(t).or_else(|| radix(t)).or_else(|| arith(t))
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
        assert!(c.top);
        assert_eq!(vals("0x1F"), vec!["31", "0001 1111"]);
        assert_eq!(c.rows[1].copy, "0b00011111");
        assert_eq!(vals("0b111011"), vec!["0x3B", "59"]);
        let c = evaluate("1187").unwrap();
        assert!(!c.top);
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
        // 進数の1187と同じ扱い(一致の下)
        assert!(!evaluate("(1+2)*3").unwrap().top);
    }

    #[test]
    fn not_calc() {
        assert!(evaluate("pwpt").is_none());
        assert!(evaluate("report 2024").is_none());
        assert!(evaluate("dB").is_none());
    }
}

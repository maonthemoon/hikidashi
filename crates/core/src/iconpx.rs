//! アイコンの絵(BGRA, 1画素4バイト)を整える. 読み方と描き方は画面の側が行い, ここは画素の計算だけ.
//! 透明の値を掛けた形(premultiplied)にそろえ, なめらかに縮め, 地に溶ける色かどうかを調べる.

/// 地に溶けやすい色か
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Normal,
    /// 白っぽいものだけ(ライトの地で見えない)
    Light,
    /// 黒っぽいものだけ(ダークの地で見えない)
    Dark,
}

/// Windowsから得た絵を, 透明の値を掛けた形にする.
/// 透明の値が全部0なら不透明な絵, 色が透明の値より大きい画素があれば(丸めの差は見ない)掛けていない絵とみなす(そのまま重ねると縁が光る)
pub fn premultiply(px: &mut [u8]) {
    if px.chunks_exact(4).all(|p| p[3] == 0) {
        px.chunks_exact_mut(4).for_each(|p| p[3] = 255);
        return;
    }
    let straight = px.chunks_exact(4).any(|p| p[..3].iter().any(|c| *c as u32 > p[3] as u32 + 4));
    for p in px.chunks_exact_mut(4) {
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = if straight { ((*c as u32 * a + 127) / 255) as u8 } else { (*c).min(a as u8) };
        }
    }
}

/// 黒の地と白の地に描いたものから, 透明の値を掛けた絵を作る(どの形のアイコンでも使える)
pub fn from_black_white(black: &[u8], white: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; black.len()];
    for ((o, b), w) in out.chunks_exact_mut(4).zip(black.chunks_exact(4)).zip(white.chunks_exact(4)) {
        let diff = (0..3).map(|i| w[i] as i32 - b[i] as i32).max().unwrap_or(0).clamp(0, 255);
        let a = (255 - diff) as u8;
        for i in 0..3 {
            o[i] = b[i].min(a);
        }
        o[3] = a;
    }
    out
}

/// `dw`×`dh`に縮める(広げる)(透明の値を掛けた絵. 面積の平均)
pub fn resize(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if (sw, sh) == (dw, dh) || sw == 0 || sh == 0 {
        return src.to_vec();
    }
    let mut out = vec![0u8; dw * dh * 4];
    let (kx, ky) = (sw as f64 / dw as f64, sh as f64 / dh as f64);
    for y in 0..dh {
        let (y0, y1) = (y as f64 * ky, (y + 1) as f64 * ky);
        for x in 0..dw {
            let (x0, x1) = (x as f64 * kx, (x + 1) as f64 * kx);
            let mut sum = [0.0f64; 4];
            let mut area = 0.0;
            for sy in y0 as usize..(y1.ceil() as usize).min(sh) {
                let wy = (y1.min(sy as f64 + 1.0) - y0.max(sy as f64)).max(0.0);
                for sx in x0 as usize..(x1.ceil() as usize).min(sw) {
                    let w = wy * (x1.min(sx as f64 + 1.0) - x0.max(sx as f64)).max(0.0);
                    let p = &src[(sy * sw + sx) * 4..][..4];
                    for i in 0..4 {
                        sum[i] += p[i] as f64 * w;
                    }
                    area += w;
                }
            }
            for i in 0..4 {
                out[(y * dw + x) * 4 + i] = (sum[i] / area.max(1e-9)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// 見えている画素のほとんどが白っぽい, または黒っぽいか
pub fn tone(px: &[u8]) -> Tone {
    let (mut all, mut light, mut dark) = (0u64, 0u64, 0u64);
    for p in px.chunks_exact(4) {
        let a = p[3] as u64;
        if a < 32 {
            continue;
        }
        let un = |c: u8| (c as u64 * 255 / a).min(255);
        let lum = (un(p[2]) * 299 + un(p[1]) * 587 + un(p[0]) * 114) / 1000;
        all += a;
        if lum >= 215 {
            light += a;
        } else if lum <= 50 {
            dark += a;
        }
    }
    match all {
        0 => Tone::Normal,
        _ if light * 100 >= all * 85 => Tone::Light,
        _ if dark * 100 >= all * 85 => Tone::Dark,
        _ => Tone::Normal,
    }
}

/// 角の丸い板の上に, 小さくした絵を重ねる(`size`四方. `rgb`は板の色)
pub fn on_plate(px: &[u8], size: usize, rgb: u32) -> Vec<u8> {
    let inner = (size * 72 / 100).max(1);
    let small = resize(px, size, size, inner, inner);
    let off = (size - inner) / 2;
    let r = size as f64 * 0.22;
    let mut out = vec![0u8; size * size * 4];
    let plate = [(rgb & 0xff) as f64, (rgb >> 8 & 0xff) as f64, (rgb >> 16 & 0xff) as f64];
    for y in 0..size {
        for x in 0..size {
            // 角の丸い四角の中か(縁は1画素でなめらかに)
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            let n = size as f64;
            let dx = (r - cx).max(cx - (n - r)).max(0.0);
            let dy = (r - cy).max(cy - (n - r)).max(0.0);
            let cover = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
            let mut p = [plate[0] * cover, plate[1] * cover, plate[2] * cover, 255.0 * cover];
            if x >= off && y >= off && x < off + inner && y < off + inner {
                let s = &small[((y - off) * inner + (x - off)) * 4..][..4];
                let k = 1.0 - s[3] as f64 / 255.0;
                for i in 0..4 {
                    p[i] = s[i] as f64 + p[i] * k;
                }
            }
            for i in 0..4 {
                out[(y * size + x) * 4 + i] = p[i].round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_alpha_is_multiplied() {
        // 白の半透明. 掛けていない形(そのまま重ねると光る)
        let mut px = vec![255, 255, 255, 128, 0, 0, 0, 0];
        premultiply(&mut px);
        assert_eq!(px, vec![128, 128, 128, 128, 0, 0, 0, 0]);
        // もう掛けてあるものは変えない
        let mut px = vec![100, 50, 0, 128, 10, 10, 10, 10];
        premultiply(&mut px);
        assert_eq!(px, vec![100, 50, 0, 128, 10, 10, 10, 10]);
        // 透明の値がない絵は不透明
        let mut px = vec![1, 2, 3, 0];
        premultiply(&mut px);
        assert_eq!(px, vec![1, 2, 3, 255]);
    }

    #[test]
    fn black_and_white_give_alpha() {
        // 不透明の赤, 透明, 半透明の白
        let black = [0, 0, 200, 0, 0, 0, 0, 0, 128, 128, 128, 0];
        let white = [0, 0, 200, 0, 255, 255, 255, 0, 255, 255, 255, 0];
        assert_eq!(from_black_white(&black, &white), vec![0, 0, 200, 255, 0, 0, 0, 0, 128, 128, 128, 128]);
    }

    #[test]
    fn resize_averages() {
        let src = [0u8, 0, 0, 0, 200, 200, 200, 200, 0, 0, 0, 0, 200, 200, 200, 200];
        assert_eq!(resize(&src, 2, 2, 1, 1), vec![100, 100, 100, 100]);
        assert_eq!(resize(&src, 2, 2, 2, 2), src.to_vec());
        assert_eq!(resize(&[9, 9, 9, 255].repeat(48 * 48), 48, 48, 28, 28), [9, 9, 9, 255].repeat(28 * 28));
    }

    #[test]
    fn tone_of_icons() {
        assert_eq!(tone(&[250, 250, 250, 255].repeat(10)), Tone::Light);
        assert_eq!(tone(&[20, 20, 20, 255].repeat(10)), Tone::Dark);
        assert_eq!(tone(&[40, 100, 220, 255].repeat(10)), Tone::Normal);
        // 白い板に色の印があるものは, そのまま見える
        let mut px = [255, 255, 255, 255].repeat(7);
        px.extend([40, 100, 220, 255].repeat(3));
        assert_eq!(tone(&px), Tone::Normal);
        // 半透明の白も白
        assert_eq!(tone(&[128, 128, 128, 128].repeat(4)), Tone::Light);
        assert_eq!(tone(&[0; 16]), Tone::Normal);
    }

    #[test]
    fn plate_is_round_and_keeps_the_mark() {
        let px = [255, 255, 255, 255].repeat(28 * 28);
        let p = on_plate(&px, 28, 0x5E5D59);
        assert_eq!(p[3], 0); // 角は透明
        let c = (14 * 28 + 14) * 4;
        assert_eq!(&p[c..c + 4], &[255, 255, 255, 255]);
        let e = (14 * 28 + 1) * 4;
        assert_eq!(&p[e..e + 4], &[0x59, 0x5D, 0x5E, 255]);
    }
}

# /// script
# requires-python = ">=3.9"
# dependencies = ["pillow"]
# ///
"""Hikidashiのアイコン(assets/hikidashi.ico)を作る. wakudoriと同じ描き方で, 色は墨(ほぼ黒)の角丸の四角に, クレイの線. 並べたときに見分けるため(K19).
座標は64×64の格子で決め, 大きさごとに8倍で描いてから縮める.

使い方:
  uv run tools/make-icon.py            決めた案でassets/hikidashi.icoを作る
  uv run tools/make-icon.py --preview DIR   全部の案をPNGで書き出す(比べる用)
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

CLAY = (0xD9, 0x77, 0x57, 255)
IVORY = (0xFA, 0xF9, 0xF5, 255)
# 地と線の色. wakudori(クレイの地にアイボリーの線)と見分けるため, 墨の地にクレイの線(案B)にした(K19)
INK = (0x14, 0x14, 0x13, 255)
BG = INK
FG = CLAY
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
SS = 8
CHOSEN = "M"


def pill(d, x1, y1, x2, y2, fill):
    """両端の丸い太い線(横向き)"""
    r = (y2 - y1) / 2
    d.rectangle([x1 + r, y1, x2 - r, y2], fill=fill)
    d.ellipse([x1, y1, x1 + 2 * r, y2], fill=fill)
    d.ellipse([x2 - 2 * r, y1, x2, y2], fill=fill)


def stroke(d, pts, w):
    """角と両端の丸い線"""
    d.line(pts, fill=FG, width=round(w), joint="curve")
    for x, y in (pts[0], pts[-1]):
        d.ellipse([x - w / 2, y - w / 2, x + w / 2, y + w / 2], fill=FG)


def draw(size: int, variant: str) -> Image.Image:
    n = size * SS
    k = n / 64.0
    img = Image.new("RGBA", (n, n), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle([2 * k, 2 * k, 62 * k, 62 * k], radius=14 * k, fill=BG)
    line = round(max(3 * k, SS * 1.0))
    knob = max(6 * k, SS * 2.0)
    if variant == "A":
        # 検索の入力欄: 横長の細い角丸の枠と, 左に虫眼鏡, 右に打った文字の線. ランチャーの入力欄の形
        small = size <= 24
        if small:
            # 小さいときは虫眼鏡を大きくし, 線を太くしてつぶれないようにする
            line = round(max(4 * k, SS * 1.5))
            d.rounded_rectangle([6 * k, 18 * k, 58 * k, 46 * k], radius=14 * k, outline=FG, width=line)
            cx, cy, r = 21 * k, 30.5 * k, 6 * k
            d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=line)
            stroke(d, [(cx + r * 0.7, cy + r * 0.7), (cx + r * 1.45, cy + r * 1.45)], line)
            pill(d, 34 * k, 32 * k - line / 2, 49 * k, 32 * k + line / 2, FG)
        else:
            d.rounded_rectangle([8 * k, 21 * k, 56 * k, 43 * k], radius=11 * k, outline=FG, width=line)
            cx, cy, r = 19 * k, 30.5 * k, 4 * k
            d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=line)
            stroke(d, [(cx + r * 0.75, cy + r * 0.75), (cx + r * 1.6, cy + r * 1.6)], line)
            pill(d, 31 * k, 32 * k - line / 2, 47 * k, 32 * k + line / 2, FG)
    elif variant == "B":
        # 虫眼鏡: 細い輪と, 太い柄(wakudoriの細い枠と太いつまみと同じ組み合わせ)
        cx, cy, r = 28 * k, 28 * k, 13 * k
        d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=line)
        w = max(6 * k, SS * 2.0)
        d.line([cx + r * 0.72, cy + r * 0.72, 48 * k, 48 * k], fill=FG, width=round(w))
        d.ellipse([48 * k - w / 2, 48 * k - w / 2, 48 * k + w / 2, 48 * k + w / 2], fill=FG)
    elif variant == "M":
        # 大きい虫眼鏡: 四角いっぱいの輪と太い柄. 16pxでも形が分かるように(K19)
        small = size <= 24
        ring = max(5 * k, SS * (1.6 if small else 1.0))
        w = max(9 * k, SS * 2.6)
        cx, cy, r = 27 * k, 27 * k, 15 * k
        d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=round(ring))
        x1, y1 = cx + r * 0.62, cy + r * 0.62
        d.line([x1, y1, 50 * k, 50 * k], fill=FG, width=round(w))
        d.ellipse([50 * k - w / 2, 50 * k - w / 2, 50 * k + w / 2, 50 * k + w / 2], fill=FG)
    elif variant in ("A3", "A4"):
        # 入力欄だけ(虫眼鏡なし): 太い枠に, 打った文字の線. A4は文字の後ろにカーソルの縦線を足す(K19)
        small = size <= 24
        line = round(SS * 1.5) if small else round(5 * k)
        if small:
            d.rounded_rectangle([4 * k, 13 * k, 60 * k, 51 * k], radius=19 * k, outline=FG, width=line)
        else:
            d.rounded_rectangle([7 * k, 15 * k, 57 * k, 49 * k], radius=17 * k, outline=FG, width=line)
        t = 2 * SS if small else 7 * k
        if variant == "A3":
            pill(d, 18 * k, 32 * k - t / 2, 46 * k, 32 * k + t / 2, FG)
        else:
            pill(d, 17 * k, 32 * k - t / 2, 36 * k, 32 * k + t / 2, FG)
            cw = line
            d.rectangle([43 * k - cw / 2, 23 * k, 43 * k + cw / 2, 41 * k], fill=FG)
    elif variant == "A2":
        # 太い入力欄: Aの形のまま, 縦に大きくして線を太くする(K19)
        if size <= 24:
            # 小さいときは枠を四角いっぱいにし, 虫眼鏡は輪と短い柄だけ, 文字の線は太く
            line = round(SS * 1.5)
            d.rounded_rectangle([4 * k, 13 * k, 60 * k, 51 * k], radius=19 * k, outline=FG, width=line)
            cx, cy, r = 21 * k, 30 * k, 7 * k
            d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=line)
            stroke(d, [(cx + r * 0.7, cy + r * 0.7), (cx + r * 1.35, cy + r * 1.35)], line)
            pill(d, 36 * k, 32 * k - SS, 52 * k, 32 * k + SS, FG)
        else:
            line = round(5 * k)
            d.rounded_rectangle([7 * k, 15 * k, 57 * k, 49 * k], radius=17 * k, outline=FG, width=line)
            cx, cy, r = 22 * k, 30 * k, 7 * k
            d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=FG, width=line)
            stroke(d, [(cx + r * 0.7, cy + r * 0.7), (cx + r * 1.5, cy + r * 1.5)], line)
            pill(d, 35 * k, 32 * k - 3.5 * k, 49 * k, 32 * k + 3.5 * k, FG)
    elif variant == "C":
        # 小さなランチャーの窓: 上に入力欄(細い枠), 下に結果の行. 選んだ行だけ太い
        d.rounded_rectangle([12 * k, 12 * k, 52 * k, 26 * k], radius=3 * k, outline=FG, width=line)
        pill(d, 12 * k, 36 * k - knob / 2, 52 * k, 36 * k + knob / 2, FG)
        pill(d, 12 * k, 47 * k - line / 2, 40 * k, 47 * k + line / 2, FG)
    elif variant == "D":
        # 打つと出る: 細い「›」と, 太いカーソル
        stroke(d, [(19 * k, 21 * k), (30 * k, 32 * k), (19 * k, 43 * k)], line)
        pill(d, 34 * k, 43 * k - knob / 2, 47 * k, 43 * k + knob / 2, FG)
    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    if len(sys.argv) >= 3 and sys.argv[1] == "--preview":
        out = Path(sys.argv[2])
        out.mkdir(parents=True, exist_ok=True)
        for v in ["A", "A2", "M", "B", "C", "D"]:
            for s in (256, 48, 32, 24, 16):
                draw(s, v).save(out / f"{v}_{s}.png")
        print(out)
        return
    out = Path(__file__).resolve().parent.parent / "assets" / "hikidashi.ico"
    images = [draw(s, CHOSEN) for s in SIZES]
    images[-1].save(out, format="ICO", sizes=[(s, s) for s in SIZES], append_images=images[:-1])
    print(f"{out} (案{CHOSEN})")


if __name__ == "__main__":
    main()

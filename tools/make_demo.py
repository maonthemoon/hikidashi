# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10"]
# ///
"""READMEとマニュアル用の紹介GIF(イメージ図)を作る.

出力: docs/images/demo.gif(README).

実際の画面は撮らず, 架空のデスクトップの上に, ランチャーの窓を
crates/app/src/win/theme.rs と paint.rs と同じ色・大きさで描く(wakudoriのtools/make_demo.pyと同じ作り).
窓の見た目を変えたら, ここの定数もそろえる.

使い方:
    uv run tools/make_demo.py [--font NotoSansJP.ttf] [--out docs/images]

フォントは Noto Sans JP(可変フォント, OFL)を使う. --font がなければ
~/.cache/hikidashi/NotoSansJP.ttf(なければ ~/.cache/wakudori/)を探し, なければ Google Fonts から取ってくる.
"""

from __future__ import annotations

import argparse
import math
import urllib.request
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

FONT_URL = "https://github.com/google/fonts/raw/main/ofl/notosansjp/NotoSansJP%5Bwght%5D.ttf"

SS = 2  # 2倍で描いてから縮める
W, H = 880, 520  # 画面
CAP = 56  # 下の説明の帯

# ---- crates/app/src/win/theme.rs ----
BG = (0xFA, 0xF9, 0xF5)
BG_SUBTLE = (0xF0, 0xEE, 0xE6)
BG_MUTED = (0xE8, 0xE6, 0xDC)
TEXT = (0x14, 0x14, 0x13)
TEXT2 = (0x3D, 0x3D, 0x3A)
TEXT3 = (0x5E, 0x5D, 0x59)
BORDER = (0xD1, 0xCF, 0xC5)
BORDER_FAINT = (0xE8, 0xE6, 0xDC)
ACCENT = (0xD9, 0x77, 0x57)
ACCENT_I = (0xB8, 0x5A, 0x39)
WIDTH, INPUT_H, PAD_X, ROW_H, LIST_PAD, ICON = 680, 60, 20, 52, 6, 28
INPUT_FONT, NAME_FONT, SUB_FONT, TAG_FONT = 20, 15, 13, 13

# 架空の色
SKY = (0x6A, 0x9B, 0xCC)
OLIVE = (0x78, 0x8C, 0x5D)
CLAY = (0xD9, 0x77, 0x57)
SAND = (0xE3, 0xC5, 0x9A)
FIG = (0xC4, 0x66, 0x86)
WIN = (0x2F, 0x2E, 0x2B)

LX = (W - WIDTH) // 2  # ランチャーの左(画面の中央)
LY = 70  # 上(中央やや上)


class Fonts:
    def __init__(self, path: Path):
        self.path = path
        self.cache: dict[tuple[int, str], ImageFont.FreeTypeFont] = {}

    def get(self, size: float, weight: str = "Regular") -> ImageFont.FreeTypeFont:
        key = (round(size * SS), weight)
        if key not in self.cache:
            f = ImageFont.truetype(str(self.path), key[0])
            try:
                f.set_variation_by_name(weight)
            except Exception:
                pass
            self.cache[key] = f
        return self.cache[key]


def s(v: float) -> int:
    return int(round(v * SS))


def box(x, y, w, h):
    return [s(x), s(y), s(x + w) - 1, s(y + h) - 1]


def layer(img: Image.Image, fn):
    lay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    fn(ImageDraw.Draw(lay))
    img.alpha_composite(lay)


def shadow(img: Image.Image, rect, radius, alpha=70, blur=16, dy=8):
    x, y, w, h = rect
    lay = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(lay).rounded_rectangle(box(x, y + dy, w, h), radius=s(radius), fill=(0x14, 0x10, 0x0C, alpha))
    img.alpha_composite(lay.filter(ImageFilter.GaussianBlur(s(blur))))


# ---------------- 架空のデスクトップ ----------------

def draw_wallpaper() -> Image.Image:
    q = 4
    small = Image.new("RGB", (W // q, H // q), (0xF2, 0xE8, 0xDA))
    d = ImageDraw.Draw(small)
    blobs = [
        (CLAY, (90, 430), 250),
        ((0xE8, 0x9F, 0x7E), (330, 520), 200),
        (SKY, (820, 70), 230),
        ((0xA9, 0xC4, 0xDE), (560, -40), 200),
        (OLIVE, (760, 500), 190),
        (SAND, (300, 120), 220),
    ]
    for color, (cx, cy), r in blobs:
        d.ellipse([(cx - r) / q, (cy - r) / q, (cx + r) / q, (cy + r) / q], fill=color)
    small = small.filter(ImageFilter.GaussianBlur(120 / q))
    return small.resize((s(W), s(H)), Image.BICUBIC).convert("RGBA")


def draw_taskbar(img: Image.Image, fonts: Fonts):
    y0 = H - 36
    region = img.crop((0, s(y0), s(W), s(H))).filter(ImageFilter.GaussianBlur(s(18)))
    img.paste(region, (0, s(y0)))
    layer(img, lambda d: d.rectangle(box(0, y0, W, 36), fill=(0xFB, 0xF8, 0xF3, 185)))
    d = ImageDraw.Draw(img)
    d.line([(0, s(y0)), (s(W), s(y0))], fill=(255, 255, 255, 255), width=SS)
    x0 = W / 2 - (6 * 32 - 12) / 2
    for ox, oy in [(0, 0), (9, 0), (0, 9), (9, 9)]:
        d.rounded_rectangle(box(x0 + 1 + ox, H - 27 + oy, 8, 8), radius=s(1.5), fill=SKY)
    for i, c in enumerate([(0x9A, 0x98, 0x90), SKY, CLAY, OLIVE, (0xA3, 0x9A, 0xB8)]):
        d.rounded_rectangle(box(x0 + (i + 1) * 32, H - 28, 20, 20), radius=s(5), fill=c)
    # トレイ: Hikidashiのアイコン(クレイ色の引き出し)と時計
    tx = W - 98
    d.rounded_rectangle(box(tx, H - 27, 18, 18), radius=s(4), fill=ACCENT)
    d.rectangle(box(tx + 4, H - 22, 10, 8), outline=BG, width=SS)
    d.line([(s(tx + 7), s(H - 18)), (s(tx + 11), s(H - 18))], fill=BG, width=SS)
    d.text((s(W - 22), s(H - 24)), "10:24", font=fonts.get(10), fill=WIN, anchor="rm")
    d.text((s(W - 22), s(H - 12)), "2026/11/25", font=fonts.get(9), fill=WIN, anchor="rm")


def draw_desktop(fonts: Fonts) -> Image.Image:
    img = draw_wallpaper()
    # 後ろの作業中のウィンドウ(白紙の文書)
    rect = (110, 150, 660, 300)
    shadow(img, rect, 10, alpha=60, blur=18, dy=8)
    d = ImageDraw.Draw(img)
    d.rounded_rectangle(box(*rect), radius=s(10), fill=(0xFB, 0xFA, 0xF7), outline=(0xE2, 0xDF, 0xD6), width=SS)
    x, y, w, h = rect
    d.rounded_rectangle(box(x + 14, y + 11, 12, 12), radius=s(3), fill=SKY)
    d.text((s(x + 34), s(y + 17)), "週報.txt - メモ帳", font=fonts.get(11), fill=WIN, anchor="lm")
    for i, ln in enumerate([0.8, 0.62, 0.7, 0.45, 0.66, 0.3]):
        d.rounded_rectangle(box(x + 30, y + 54 + i * 26, (w - 60) * ln, 8), radius=s(4), fill=(0xE6, 0xE3, 0xDB))
    draw_taskbar(img, fonts)
    return img


# ---------------- アイコン(架空の形) ----------------

def draw_icon(d: ImageDraw.ImageDraw, fonts: Fonts, kind: str, x: float, y: float):
    """kind: app:<色>:<字>, doc, ppt, folder, chat"""
    if kind.startswith("app"):
        _, color, ch = kind.split(":")
        c = {"clay": CLAY, "sky": SKY, "olive": OLIVE, "fig": FIG}[color]
        d.rounded_rectangle(box(x + 1, y + 1, 26, 26), radius=s(6), fill=c)
        d.text((s(x + 14), s(y + 14)), ch, font=fonts.get(14, "Bold"), fill=(255, 255, 255), anchor="mm")
    elif kind in ("doc", "ppt", "txt"):
        c = {"doc": SKY, "ppt": CLAY, "txt": (0x9A, 0x98, 0x90)}[kind]
        d.rounded_rectangle(box(x + 5, y + 2, 19, 24), radius=s(3), fill=(255, 255, 255), outline=BORDER, width=SS)
        d.rounded_rectangle(box(x + 2, y + 13, 14, 10), radius=s(2), fill=c)
        for k in range(3):
            d.line([(s(x + 18), s(y + 8 + k * 4)), (s(x + 21), s(y + 8 + k * 4))], fill=BORDER, width=SS)
    elif kind == "folder":
        d.rounded_rectangle(box(x + 2, y + 6, 11, 6), radius=s(2), fill=(0xD9, 0xB0, 0x6C))
        d.rounded_rectangle(box(x + 2, y + 9, 24, 16), radius=s(3), fill=SAND)
    elif kind == "chat":
        d.rounded_rectangle(box(x + 2, y + 4, 24, 17), radius=s(6), outline=TEXT2, width=s(1.6))
        d.polygon([(s(x + 8), s(y + 20)), (s(x + 8), s(y + 26)), (s(x + 14), s(y + 20))], fill=TEXT2)
        for k in range(3):
            d.ellipse(box(x + 8.5 + k * 5, y + 11.5, 2.6, 2.6), fill=TEXT2)


# ---------------- ランチャーの窓(paint.rsと同じ) ----------------

def text_w(font, t: str) -> float:
    return font.getlength(t) / SS


def draw_name(d, fonts: Fonts, name: str, bold: set[int], x: float, y: float):
    for i, ch in enumerate(name):
        f = fonts.get(NAME_FONT, "Bold" if i in bold else "Regular")
        d.text((s(x), s(y)), ch, font=f, fill=TEXT, anchor="lm")
        x += text_w(f, ch)


def launcher_height(nrows: int) -> int:
    return INPUT_H + (LIST_PAD * 2 + ROW_H * nrows if nrows else 0)


def draw_launcher(img: Image.Image, fonts: Fonts, text: str, rows: list[dict], glow: float, caret: bool):
    h = launcher_height(len(rows))
    rect = (LX, LY, WIDTH, h)
    shadow(img, rect, 8, alpha=90, blur=22, dy=10)
    d = ImageDraw.Draw(img)
    d.rounded_rectangle(box(*rect), radius=s(8), fill=BG, outline=BORDER, width=SS)

    # 入力欄
    f = fonts.get(INPUT_FONT)
    ty = LY + INPUT_H / 2
    if text:
        d.text((s(LX + PAD_X), s(ty)), text, font=f, fill=TEXT, anchor="lm")
    else:
        d.text((s(LX + PAD_X), s(ty)), "Search apps, files, bookmarks, or paste a path", font=f, fill=TEXT3, anchor="lm")
    if caret:
        cx = LX + PAD_X + (text_w(f, text) if text else 0) + 1
        d.line([(s(cx), s(ty - 12)), (s(cx), s(ty + 12))], fill=TEXT, width=SS)
    # 下の線. 打つたびに一瞬クレイ色に光る
    line = tuple(int(round(a + (b - a) * glow)) for a, b in zip(BORDER_FAINT, ACCENT))
    d.rectangle(box(LX + 1, LY + INPUT_H - (2 if glow > 0 else 1), WIDTH - 2, 2 if glow > 0 else 1), fill=line)

    y = LY + INPUT_H + LIST_PAD
    for i, r in enumerate(rows):
        if i == 0:
            d.rounded_rectangle(box(LX + LIST_PAD, y, WIDTH - LIST_PAD * 2, ROW_H), radius=s(8), fill=BG_MUTED)
        ix = LX + LIST_PAD + 12
        draw_icon(d, fonts, r["icon"], ix, y + (ROW_H - ICON) / 2)
        tf = fonts.get(TAG_FONT)
        tag_color = ACCENT_I if r.get("warn") else TEXT3
        d.text((s(LX + WIDTH - LIST_PAD - 12), s(y + ROW_H / 2)), r["tag"], font=tf, fill=tag_color, anchor="rm")
        nx = ix + ICON + 12
        if r.get("sub"):
            draw_name(d, fonts, r["name"], r.get("bold", set()), nx, y + ROW_H / 2 - 9)
            d.text((s(nx), s(y + ROW_H / 2 + 11)), r["sub"], font=fonts.get(SUB_FONT), fill=TEXT3, anchor="lm")
        else:
            draw_name(d, fonts, r["name"], r.get("bold", set()), nx, y + ROW_H / 2)
        y += ROW_H


def draw_browser(img: Image.Image, fonts: Fonts, question: str):
    """ChatGPTに送ったあとの, ブラウザーの窓(架空)."""
    rect = (150, 60, 580, 360)
    shadow(img, rect, 10, alpha=80, blur=20, dy=10)
    d = ImageDraw.Draw(img)
    x, y, w, h = rect
    d.rounded_rectangle(box(*rect), radius=s(10), fill=(255, 255, 255), outline=(0xE2, 0xDF, 0xD6), width=SS)
    d.rounded_rectangle(box(x + 12, y + 10, w - 24, 22), radius=s(11), fill=BG_SUBTLE)
    d.text((s(x + 26), s(y + 21)), "chatgpt.com", font=fonts.get(11), fill=TEXT3, anchor="lm")
    f = fonts.get(13)
    bw = text_w(f, question) + 28
    d.rounded_rectangle(box(x + w - 24 - bw, y + 60, bw, 34), radius=s(17), fill=BG_SUBTLE)
    d.text((s(x + w - 24 - bw + 14), s(y + 77)), question, font=f, fill=TEXT, anchor="lm")
    for i, ln in enumerate([0.86, 0.74, 0.8, 0.5]):
        d.rounded_rectangle(box(x + 28, y + 122 + i * 24, (w - 56) * ln, 8), radius=s(4), fill=(0xEC, 0xEA, 0xE3))
    d.rounded_rectangle(box(x + 28, y + 236, w - 56, 26), radius=s(6), fill=(0xF4, 0xF2, 0xEC))
    d.text((s(x + 40), s(y + 249)), "git branch -m new-name", font=fonts.get(11), fill=TEXT2, anchor="lm")


def draw_keycap(img: Image.Image, fonts: Fonts, label: str, pressed: bool):
    font = fonts.get(15, "Medium")
    w = text_w(font, label) + 36
    x, y, h = W / 2 - w / 2, H - 92, 38
    off = 2 if pressed else 0

    def fn(d):
        d.rounded_rectangle(box(x, y + 3, w, h), radius=s(8), fill=BORDER)
        d.rounded_rectangle(box(x, y + off, w, h), radius=s(8), fill=(255, 255, 255), outline=BORDER, width=SS)
        d.text((s(x + w / 2), s(y + off + h / 2)), label, font=font, fill=TEXT, anchor="mm")

    layer(img, fn)


def draw_caption(img: Image.Image, fonts: Fonts, step: int, text: str):
    d = ImageDraw.Draw(img)
    y0 = H
    d.rectangle(box(0, y0, W, CAP), fill=BG)
    d.line([(0, s(y0)), (s(W), s(y0))], fill=BG_MUTED, width=SS)
    if step:
        d.ellipse(box(20, y0 + CAP / 2 - 12, 24, 24), fill=ACCENT_I)
        d.text((s(32), s(y0 + CAP / 2)), str(step), font=fonts.get(13, "Bold"), fill=(255, 255, 255), anchor="mm")
    d.text((s(56), s(y0 + CAP / 2)), text, font=fonts.get(15, "Medium"), fill=TEXT, anchor="lm")
    d.text((s(W - 20), s(y0 + CAP / 2)), "イメージ(架空の画面)", font=fonts.get(11), fill=TEXT3, anchor="rm")


# ---------------- 筋書き ----------------

DOCS = r"C:\Users\me\Documents"

SCENES = [
    dict(
        cap=(1, "飛び飛びの文字で探す: pwpt → PowerPoint"),
        text="pwpt",
        rows=[
            dict(icon="app:clay:P", name="PowerPoint", bold={0, 2, 5, 9}, tag="app"),
            dict(icon="ppt", name="提案_PowerPoint用テンプレート.potx", bold={3, 5, 8, 12}, sub=DOCS + r"\テンプレート", tag="file"),
        ],
        result=None,
    ),
    dict(
        cap=(2, "ローマ字で日本語の名前を探す: kaigi → 会議"),
        text="kaigi",
        rows=[
            dict(icon="doc", name="定例会議_議事録.docx", bold={2, 3}, sub=DOCS + r"\会議", tag="file"),
            dict(icon="ppt", name="会議資料_11月.pptx", bold={0, 1}, sub=DOCS + r"\プロジェクトA", tag="file"),
            dict(icon="folder", name="会議", bold={0, 1}, sub=DOCS, tag="folder"),
        ],
        result=None,
    ),
    dict(
        cap=(3, "パスを貼ってEnterで開く(引用符や/mnt/cの形も)"),
        text='"' + DOCS + r'\報告書.docx"',
        paste=True,
        rows=[dict(icon="doc", name="報告書.docx", sub=DOCS, tag="file")],
        result=None,
    ),
    dict(
        cap=(4, "見つからなければ, そのままChatGPTに聞く"),
        text="how to rename a git branch",
        rows=[dict(icon="chat", name="how to rename a git branch", sub="No results · Ctrl+Alt+Enter for Google", tag="ChatGPT")],
        result="browser",
    ),
]


def storyboard():
    """(状態, 表示する時間ms)を順に返す."""
    out = []
    for k, sc in enumerate(SCENES):
        base = dict(show=False, text="", rows=[], glow=0.0, caret=False, key=None, cap=sc["cap"], result=None, scene=k)

        def st(**kw):
            d = dict(base)
            d.update(kw)
            return d

        out.append((st(), 500 if k else 900))
        out.append((st(key="Alt+Space"), 350))
        out.append((st(key="Alt+Space!"), 200))
        out.append((st(show=True, caret=True), 500))
        if sc.get("paste"):
            out.append((st(show=True, caret=True, key="Ctrl+V"), 350))
            out.append((st(show=True, caret=True, key="Ctrl+V!"), 200))
            out.append((st(show=True, text=sc["text"], rows=sc["rows"], glow=1.0, caret=True), 120))
            out.append((st(show=True, text=sc["text"], rows=sc["rows"], glow=0.4, caret=True), 120))
        else:
            for i in range(1, len(sc["text"]) + 1):
                t = sc["text"][:i]
                # 打っている途中は, 最後まで打ったときの行を出す(短い語なので見た目の差は小さい)
                rows = sc["rows"] if i == len(sc["text"]) else []
                out.append((st(show=True, text=t, rows=rows, glow=1.0, caret=True), 70 if len(sc["text"]) > 10 else 130))
        out.append((st(show=True, text=sc["text"], rows=sc["rows"], caret=True), 1700 if k != 3 else 1500))
        out.append((st(show=True, text=sc["text"], rows=sc["rows"], caret=True, key="Enter"), 350))
        out.append((st(show=True, text=sc["text"], rows=sc["rows"], caret=True, key="Enter!"), 200))
        if sc["result"] == "browser":
            out.append((st(result="browser"), 2600))
        else:
            out.append((st(), 300))
    return out


def render(fonts: Fonts, desktop: Image.Image, state) -> Image.Image:
    img = Image.new("RGBA", (s(W), s(H + CAP)), BG + (255,))
    screen = desktop.copy()
    if state["result"] == "browser":
        draw_browser(screen, fonts, SCENES[3]["text"])
    if state["show"]:
        draw_launcher(screen, fonts, state["text"], state["rows"], state["glow"], state["caret"])
    if state["key"]:
        draw_keycap(screen, fonts, state["key"].rstrip("!"), state["key"].endswith("!"))
    img.paste(screen, (0, 0))
    draw_caption(img, fonts, *state["cap"])
    return img.resize((W, H + CAP), Image.LANCZOS).convert("RGB")


def find_font(arg: str | None) -> Path:
    if arg:
        return Path(arg)
    for name in ("hikidashi", "wakudori"):
        p = Path.home() / ".cache" / name / "NotoSansJP.ttf"
        if p.exists():
            return p
    cached = Path.home() / ".cache" / "hikidashi" / "NotoSansJP.ttf"
    cached.parent.mkdir(parents=True, exist_ok=True)
    print(f"フォントを取得: {FONT_URL}")
    urllib.request.urlretrieve(FONT_URL, cached)
    return cached


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--font")
    ap.add_argument("--out", default=str(Path(__file__).resolve().parent.parent / "docs" / "images"))
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    fonts = Fonts(find_font(args.font))
    desktop = draw_desktop(fonts)
    board = storyboard()
    frames = [render(fonts, desktop, st) for st, _ in board]
    durations = [ms for _, ms in board]

    # 全コマで同じパレットを使う(コマごとに色がちらつかないように)
    picks = [i for i, (st, _) in enumerate(board) if st["rows"] and not st["key"] and st["glow"] == 0.0]
    picks += [i for i, (st, _) in enumerate(board) if st["result"]][:1]
    sheet = Image.new("RGB", (W, (H + CAP) * len(picks)))
    for k, idx in enumerate(picks):
        sheet.paste(frames[idx], (0, (H + CAP) * k))
    palette = sheet.quantize(colors=255, method=Image.Quantize.MEDIANCUT)
    pframes = [f.quantize(palette=palette, dither=Image.Dither.FLOYDSTEINBERG) for f in frames]
    gif = out / "demo.gif"
    pframes[0].save(gif, save_all=True, append_images=pframes[1:], duration=durations, loop=0, optimize=True)

    print(f"{gif} ({gif.stat().st_size // 1024} KB, {len(frames)}コマ, {sum(durations) / 1000:.1f}秒)")


if __name__ == "__main__":
    main()

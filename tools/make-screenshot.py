# /// script
# requires-python = ">=3.10"
# dependencies = ["playwright>=1.48"]
# ///
"""READMEの先頭に置く静止画(docs/images/hikidashi.png)を, 紹介のページ(docs/index.html)から撮る.

ページの動くデモを「ローマ字で日本語」の場面まで進め, 見出しからデモの下までを2倍の解像度で撮る.
使い方: uv run tools/make-screenshot.py  (初めてのときは uv run --with playwright playwright install chromium)
"""
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images" / "hikidashi.png"

with sync_playwright() as p:
    b = p.chromium.launch()
    pg = b.new_page(viewport={"width": 1280, "height": 900}, device_scale_factor=2, reduced_motion="no-preference")
    pg.goto((ROOT / "docs" / "index.html").as_uri())
    pg.locator(".demo-tab", has_text="ローマ字で日本語").click()
    pg.wait_for_function("document.querySelectorAll('.win .row').length >= 3 && document.querySelector('.win__text')?.textContent === 'kaigi'", timeout=20000)
    time.sleep(0.8)  # 入力欄の下の線が光り終わるまで
    box = pg.locator(".desk").bounding_box()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    pg.screenshot(path=str(OUT), clip={"x": 0, "y": 0, "width": 1280, "height": box["y"] + box["height"] + 24})
    b.close()
print(OUT)

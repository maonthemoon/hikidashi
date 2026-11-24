#!/bin/sh
# マニュアル(docs/manual.html)と紹介のページ(docs/index.html. GitHub Pagesのトップ)を,
# 開発用の文書の元のファイル(dev/docs/manual-src/, dev/docs/site-src/)から作り直す.
# anthropic-htmlスキルのbuild_html.pyでCSSを埋め込み, 見出しの明朝を使う文字だけ埋め込む.
# 公開するファイルのCSSのコメントから, 他社のサイト名と出どころの記述を消す(wakudoriのG2と同じ).
set -eu
cd "$(dirname "$0")/.."
SKILL="${ANTHROPIC_HTML:-$HOME/.claude/skills/anthropic-html}"
uv run "$SKILL/scripts/build_html.py" dev/docs/manual-src/index.html docs/manual.html
uv run "$SKILL/scripts/build_html.py" dev/docs/site-src/index.html docs/index.html
python3 - <<'PY'
import re
words = r"(?:Anthropic|anthropic|Claude|公開CSS|research/)"
for p in ["docs/manual.html", "docs/index.html"]:
    s = open(p, encoding="utf-8").read()
    s = re.sub(r"/\*(?:(?!\*/)[\s\S])*?" + words + r"(?:(?!\*/)[\s\S])*?\*/", "", s)
    open(p, "w", encoding="utf-8").write(s)
    left = re.findall(r"(?i)anthropic|claude", s)
    print(p + ": 他社名の残り", len(left), "件")
PY

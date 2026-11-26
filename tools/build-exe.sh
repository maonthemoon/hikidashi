#!/bin/sh
# Linux(Raspberry Piなど)で, Windows用のHikidashi.exeを作る. テストも先に通す.
# 出す場所は out/(手元での確かめ用). bin/ はリリースの版だけ(Actionsが置く). 会社PCではリリースの版で確かめる.
set -eu
cd "$(dirname "$0")/.."
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
"$CARGO" test -q
# トレイのManualはdocs/manual.htmlを開く(K46). 元のファイルの方が新しければ作り直す
if [ -f dev/docs/manual-src/index.html ] && [ dev/docs/manual-src/index.html -nt docs/manual.html ]; then
  ./tools/build-manual.sh
fi
# exeの中のエラー文に手元のパス(ユーザー名を含む)が残らないよう置き換える. 後に書いたものが優先される
RUSTFLAGS="--remap-path-prefix=$HOME=/home --remap-path-prefix=$HOME/.cargo/registry/src=/cargo --remap-path-prefix=$PWD=/hikidashi" \
  "$CARGO" build --release --target x86_64-pc-windows-gnu -p hikidashi
OUT=out
mkdir -p "$OUT"
cp target/x86_64-pc-windows-gnu/release/hikidashi.exe "$OUT/Hikidashi.exe"
# 辞書はexeとは別のファイルとして隣に置く(ZIPと同じ形. K35). 開発用の文書にUTF-8版があれば使う
if [ -f dev/dict/SKK-JISYO.L.utf8 ]; then
  cmp -s dev/dict/SKK-JISYO.L.utf8 "$OUT/SKK-JISYO.L" || cp dev/dict/SKK-JISYO.L.utf8 "$OUT/SKK-JISYO.L"
fi
ls -l "$OUT/Hikidashi.exe"

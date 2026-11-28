#!/bin/sh
# 辞書(SKK-JISYO.L. GPL)とそのライセンスの本文を取ってきて data/ に置く(K35, K36, K82).
# exeとは別のファイルとして配る(gitで入れた人もZIPの人も, bin/の隣のdata/に辞書がある). 辞書を新しくしたいときに実行してcommitする.
set -eu
cd "$(dirname "$0")/.."
mkdir -p data
curl -fsSL https://raw.githubusercontent.com/skk-dev/dict/master/SKK-JISYO.L -o data/SKK-JISYO.L.tmp
head -c 100 data/SKK-JISYO.L.tmp | grep -q "coding" || { echo "SKK-JISYO.L looks wrong" >&2; exit 1; }
[ "$(wc -c < data/SKK-JISYO.L.tmp)" -gt 1000000 ] || { echo "SKK-JISYO.L is too small" >&2; exit 1; }
mv data/SKK-JISYO.L.tmp data/SKK-JISYO.L
{
  echo "SKK-JISYO.L is from https://github.com/skk-dev/dict and is licensed under the"
  echo "GNU General Public License version 2 or later (see the top of SKK-JISYO.L)."
  echo "It is a separate file; Hikidashi.exe reads it at run time and is under the MIT License."
  echo "The full text of the GNU General Public License version 2 follows."
  echo
  curl -fsSL https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt
} > data/SKK-JISYO.L-COPYING.txt
ls -l data/SKK-JISYO.L data/SKK-JISYO.L-COPYING.txt

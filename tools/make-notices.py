# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
"""THIRD-PARTY-NOTICES.txt を作る. exeに入るクレート(Windows向けの通常の依存. マクロのクレートは除く)の
ライセンス本文と, マニュアルと紹介のページに埋め込んだフォント, SQLite, 取ってくる辞書の表記をまとめる.

使い方: uv run tools/make-notices.py  (リポジトリの直下で. cargoが要る)
"""
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "THIRD-PARTY-NOTICES.txt"
TARGET = "x86_64-pc-windows-gnu"
OWN = {"hikidashi", "hikidashi-core", "hikidashi-ext"}
# 2つ以上から選べるときはMITを選ぶ(本文が短く, 著作権者の行がある)
PREFER = ["MIT", "Zlib", "Apache-2.0"]
OFL = Path(os.environ.get("ANTHROPIC_HTML", Path.home() / ".claude/skills/anthropic-html")) / "assets/fonts/OFL.txt"


def cargo_metadata():
    cargo = os.environ.get("CARGO", str(Path.home() / ".cargo/bin/cargo"))
    out = subprocess.run(
        [cargo, "metadata", "--format-version", "1", "--filter-platform", TARGET],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(out)


def shipped_packages(meta):
    """exeに入るパッケージ. ビルド時だけ使うもの(build-dependencies, proc-macro)はたどらない"""
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = next(p["id"] for p in meta["packages"] if p["name"] == "hikidashi")
    seen, stack = set(), [root]
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        p = pkgs[i]
        if any("proc-macro" in t["kind"] for t in p["targets"]):
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                stack.append(d["pkg"])
    return sorted((pkgs[i] for i in seen if pkgs[i]["name"] not in OWN), key=lambda p: (p["name"], p["version"]))


def choose(expr):
    names = [s.strip("() ") for s in expr.replace("/", " OR ").split(" OR ")]
    for want in PREFER:
        if want in names:
            return want
    return names[0]


def license_text(pkg, lic):
    d = Path(pkg["manifest_path"]).parent
    files = sorted(f for f in d.iterdir() if f.is_file() and f.name.upper().startswith(("LICENSE", "LICENCE", "COPYING")))
    key = {"MIT": "MIT", "Apache-2.0": "APACHE", "Zlib": "ZLIB"}.get(lic, lic.upper())
    hit = [f for f in files if key in f.name.upper()] or ([files[0]] if len(files) == 1 else [])
    if not files and pkg["name"].startswith("rquickjs-"):
        # rquickjs-core, rquickjs-sysはライセンスの本文を持たない. 同じリポジトリのrquickjsのものを使う
        sib = d.parent / f"rquickjs-{pkg['version']}" / "LICENSE"
        if sib.is_file():
            return sib.read_text(encoding="utf-8").strip()
    if not hit:
        sys.exit(f"{pkg['name']}: {lic}のライセンスファイルが見つからない({[f.name for f in files]})")
    return hit[0].read_text(encoding="utf-8").strip()


def main():
    pkgs = shipped_packages(cargo_metadata())
    groups = {}  # 本文 → [(名前 版, 選んだライセンス, 元のライセンス, リポジトリ)]
    for p in pkgs:
        lic = choose(p["license"])
        groups.setdefault(license_text(p, lic), []).append((f"{p['name']} {p['version']}", lic, p["license"], p.get("repository") or ""))

    bar = "=" * 72
    out = [
        "Third-party notices for Hikidashi",
        "",
        "Hikidashi itself is under the MIT License (see LICENSE).",
        "Hikidashi.exe contains the following software. Each is listed with the license",
        "under which it is used here. Where a crate offers a choice, the MIT License is used.",
        "",
    ]
    for text, users in groups.items():
        out.append(bar)
        for name, lic, orig, repo in users:
            out.append(f"{name}  ({lic}; offered as {orig})  {repo}".rstrip())
        out += [bar, "", text, ""]

    # QuickJS(rquickjs-sysに同梱. JavaScriptの拡張を動かす)
    sys_pkg = next((p for p in pkgs if p["name"] == "rquickjs-sys"), None)
    if sys_pkg:
        qjs = Path(sys_pkg["manifest_path"]).parent / "quickjs" / "LICENSE"
        out += [bar, "QuickJS (bundled through rquickjs-sys)  https://github.com/quickjs-ng/quickjs", bar, "", qjs.read_text(encoding="utf-8").strip(), ""]

    out += [
        bar,
        "SQLite (bundled through libsqlite3-sys)  https://sqlite.org/",
        bar,
        "",
        "SQLite is in the public domain. See https://sqlite.org/copyright.html",
        "",
        bar,
        "Fonts embedded in the manual (docs/manual.html, also in the ZIP) and the introduction page (docs/index.html), as subsets",
        "Noto Serif JP  https://github.com/notofonts/noto-cjk",
        "Noto Sans JP  https://github.com/notofonts/noto-cjk",
        "Noto Sans Mono  https://github.com/notofonts/latin-greek-cyrillic",
        bar,
        "",
        "Noto Serif JP: Copyright 2017-2024 Adobe (http://www.adobe.com/).",
        "Noto Sans JP: (c) 2014-2021 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'.",
        "Noto Sans Mono: Copyright 2022 The Noto Project Authors (https://github.com/notofonts/latin-greek-cyrillic).",
        "",
        "All three are licensed under the SIL Open Font License, Version 1.1:",
        "",
        OFL.read_text(encoding="utf-8").strip(),
        "",
        bar,
        "SKK-JISYO.L (a separate file in bin/, not inside the exe)  https://github.com/skk-dev/dict",
        bar,
        "",
        "bin/ (in the ZIP and in the repository) contains SKK-JISYO.L from the repository above as a separate file, with",
        "SKK-JISYO.L-COPYING.txt. Hikidashi.exe does not contain it; it reads the file at run",
        "time (and downloads it from the repository above if the file is missing). The",
        "dictionary is under the GNU General Public License version 2 or later; its copyright",
        "holders and license are written at the top of the file.",
        "",
    ]
    OUT.write_text("\n".join(out), encoding="utf-8")
    print(f"{OUT.name}: {len(pkgs)} crates, {len(groups)} license texts")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""仕様ルール表とテストの 1 対 1 対応を検査する。

各 crate の lib.rs にある `//! | ID | ルール |` 表の ID（例: BMS-RND-L2-01）ごとに、
`#[rstest]` のテスト関数で名前が ID の小文字・`_` 区切り（例: bms_rnd_l2_01_…）で
始まるものがちょうど 1 つあること、また対応するルールの無い rstest 関数が無いことを確かめる。

使い方: python3 tools/check_rules.py   （リポジトリ直下で実行）
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ok = True
for lib in sorted(ROOT.glob("*/crates/**/src/lib.rs")):
    text = lib.read_text(encoding="utf-8")
    if "# 仕様ルール" not in text:
        continue
    ids = re.findall(r"^//! \| ([A-Z0-9]+(?:-[A-Z0-9]+)+) \|", text, re.M)
    src = "".join(p.read_text(encoding="utf-8") for p in lib.parent.rglob("*.rs"))
    test_fns = set()
    for chunk in src.split("#[rstest]")[1:]:
        m = re.search(r"\bfn ([a-z0-9_]+)\s*\(", chunk)
        if m:
            test_fns.add(m.group(1))
    prefixes = {i: i.lower().replace("-", "_") + "_" for i in ids}
    rel = lib.relative_to(ROOT)
    for i, p in prefixes.items():
        hits = sorted(f for f in test_fns if f.startswith(p))
        if len(hits) != 1:
            ok = False
            print(f"NG {rel}: {i} のテスト関数が {len(hits)} 個 {hits}")
    for f in sorted(test_fns):
        if not any(f.startswith(p) for p in prefixes.values()):
            ok = False
            print(f"NG {rel}: テスト {f} に対応するルールが無い")
    print(f"{rel.parts[-3]}: ルール {len(ids)} / テスト関数 {len(test_fns)}")
sys.exit(0 if ok else 1)

#!/usr/bin/env python3
"""japanese-rules.md に照らして日本語の文書を検査する。

Claude (opus、effort high)を 2 つ独立に起動して同じ本文をレビューさせ、2 つの結果の和集合を違反として報告する。
文書を書いた本人は、報告を参考にして自分で直す。直した後の再検査は行わない (開発のルールの「文書の規則」の 4 番目)。
違反があれば終了ステータス 1 で終わる。

使い方:
  scripts/check-japanese.py [--full] [--staged] [--message] FILE...
    --full     git の差分ではなくファイル全体を Claude に渡す (git の管理外なら常に全体)。
    --staged   git の index に入っている差分を対象にする (pre-commit 用)。
    --message  FILE をコミットメッセージとして扱う (commit-msg 用)。
    --report=PATH  違反の一覧を JSON でも PATH に書く (文書を書いた本人が全件を読んで直すため)。
    --base=DIR     git の代わりに、DIR にある同じ名前のファイルとの差分を対象にする (git の履歴が無いとき用)。
"""
import difflib
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent
RULES_PATH = PROJECT_ROOT / "japanese-rules.md"
HEADLESS = Path.home() / ".claude/skills/claude-headless/scripts/claude_headless.py"
MODEL = "opus"
EFFORT = "high"
REVIEWERS = 2


def changed_lines_from_git(path: Path, staged: bool) -> set[int] | None:
    """git の差分から、変更された行の番号を返す。差分が取れなければ None。"""
    try:
        args = ["git", "diff", "--cached" if staged else "HEAD", "-U0", "--", str(path)]
        diff = subprocess.run(args, cwd=PROJECT_ROOT, capture_output=True, text=True, check=True).stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None
    changed = set()
    for m in re.finditer(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", diff, re.M):
        start, count = int(m.group(1)), int(m.group(2) or "1")
        changed.update(range(start, start + max(count, 1)))
    return changed


def changed_lines_from_base(path: Path, base_dir: Path) -> set[int] | None:
    """DIR にある同じ名前のファイルとの差分から、変更された行の番号を返す。基準のファイルが無ければ None。"""
    base = base_dir / path.name
    if not base.exists():
        return None
    old = base.read_text(encoding="utf-8").split("\n")
    new = path.read_text(encoding="utf-8").split("\n")
    changed = set()
    for tag, _, _, j1, j2 in difflib.SequenceMatcher(a=old, b=new, autojunk=False).get_opcodes():
        if tag in ("replace", "insert"):
            changed.update(range(j1 + 1, j2 + 1))
    return changed


def changed_paragraphs(path: Path, staged: bool, base_dir: Path | None) -> str | None:
    """変更された行を含む段落だけを、元の行番号付きで返す。差分が取れなければ None。"""
    changed = changed_lines_from_base(path, base_dir) if base_dir else changed_lines_from_git(path, staged)
    if changed is None:
        return None
    if not changed:
        return ""
    lines = path.read_text(encoding="utf-8").split("\n")
    # 段落 (空行で区切られた範囲)のうち、変更された行を含むものを集める。
    paragraphs, current = [], []
    for i, line in enumerate(lines, start=1):
        if line.strip() == "":
            if current:
                paragraphs.append(current)
                current = []
        else:
            current.append(i)
    if current:
        paragraphs.append(current)
    selected = [p for p in paragraphs if changed.intersection(p)]
    return "\n\n".join("\n".join(f"{i}: {lines[i - 1]}" for i in p) for p in selected)


def numbered(text: str) -> str:
    return "\n".join(f"{i}: {line}" for i, line in enumerate(text.split("\n"), start=1))


def build_prompt(rules: str, body: str, is_message: bool) -> str:
    kind = "git のコミットメッセージ" if is_message else "Markdown の文書 (行番号付き)"
    return f"""あなたは日本語の技術文書の校閲者である。
次の規則に反する箇所を、与えられた本文から探して報告する。

# 規則 (japanese-rules.md の全文)

{rules}

# 本文 ({kind})

{body}

# 報告の形式

規則に反する箇所だけを、JSON の配列として出力する。配列の要素は次のオブジェクトである。
{{"line": 行番号 (整数), "rule": 反している規則を規則の文から引用した文字列, "quote": 本文から該当する箇所をそのまま引用した文字列, "reason": なぜ反しているかの説明}}

- 規則に反すると確信できる箇所だけを報告する。好みや文体の改善の提案は報告しない。
- 規則に反する箇所が無ければ、空の配列 [] だけを出力する。
- JSON 以外の文字 (前置き、説明、コードブロックの記号)を出力しない。
- 「主語、述語、接続語、目的語、補語、その他の修飾語を一切省略していない」の規則は、Markdown の見出し、箇条書きの項目名 (コロンの前の語)、コードブロックの中、表の見出しには適用しない。本文の文にだけ適用する。
"""


def run_reviewer(prompt: str, index: int) -> list[dict]:
    proc = subprocess.run(
        [sys.executable, str(HEADLESS), "--model", MODEL, "--effort", EFFORT],
        input=prompt, capture_output=True, text=True,
    )
    out = proc.stdout
    start, end = out.find("["), out.rfind("]")
    if proc.returncode != 0 or start < 0 or end < 0:
        sys.stderr.write(f"レビュー {index} が失敗した (終了ステータス {proc.returncode}):\n{out[-2000:]}\n{proc.stderr[-2000:]}\n")
        return [{"line": 0, "rule": "レビューの失敗", "quote": "", "reason": f"レビュー {index} の出力を解析できなかった"}]
    try:
        items = json.loads(out[start:end + 1])
    except json.JSONDecodeError as e:
        sys.stderr.write(f"レビュー {index} の JSON を解析できなかった: {e}\n{out[-2000:]}\n")
        return [{"line": 0, "rule": "レビューの失敗", "quote": "", "reason": f"レビュー {index} の JSON を解析できなかった"}]
    for item in items:
        item["reason"] = f"レビュー {index}: {item.get('reason', '')}"
    return items


def llm_check(rules: str, body: str, is_message: bool) -> list[dict]:
    prompt = build_prompt(rules, body, is_message)
    with ThreadPoolExecutor(max_workers=REVIEWERS) as pool:
        results = list(pool.map(lambda i: run_reviewer(prompt, i), range(1, REVIEWERS + 1)))
    merged, seen = [], set()
    for items in results:
        for item in items:
            key = (item.get("line"), str(item.get("rule", ""))[:40])
            if key in seen:
                continue
            seen.add(key)
            merged.append(item)
    return sorted(merged, key=lambda v: (v.get("line") or 0))


def main(argv: list[str]) -> int:
    full = "--full" in argv
    staged = "--staged" in argv
    is_message = "--message" in argv
    report = next((a.split("=", 1)[1] for a in argv if a.startswith("--report=")), None)
    base_dir = next((Path(a.split("=", 1)[1]) for a in argv if a.startswith("--base=")), None)
    files = [Path(a) for a in argv if not a.startswith("--")]
    if not files:
        sys.stderr.write(__doc__)
        return 2
    rules = RULES_PATH.read_text(encoding="utf-8")
    total = 0
    all_violations = []
    for path in files:
        text = path.read_text(encoding="utf-8")
        if is_message:
            text = "\n".join(l for l in text.split("\n") if not l.startswith("#"))
        violations: list[dict] = []
        if is_message or full:
            body = numbered(text)
        else:
            body = changed_paragraphs(path, staged, base_dir)
            if body is None:
                body = numbered(text)
        if body.strip():
            violations = llm_check(rules, body, is_message)
        violations.sort(key=lambda v: (v.get("line") or 0))
        for v in violations:
            print(f"{path}:{v.get('line')}: {v.get('rule')}\n    該当: {v.get('quote')}\n    理由: {v.get('reason')}")
        total += len(violations)
        all_violations += [dict(v, file=str(path)) for v in violations]
    if report:
        Path(report).write_text(json.dumps(all_violations, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"違反: {total} 件")
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

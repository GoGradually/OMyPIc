#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT_DIR"

python3 - <<'PY'
from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path

TARGET_DIRS = [
    Path("application/src/main/java"),
    Path("domain/src/main/java"),
    Path("infrastructure/src/main/java"),
    Path("presentation/src/main/java"),
]
LIMIT = 10

CONTROL_KEYWORDS = {
    "if", "for", "while", "switch", "catch", "try", "else", "do", "synchronized", "return", "throw"
}
TYPE_DECLARATION_TOKENS = {"class", "interface", "enum", "record"}

METHOD_RE = re.compile(
    r"^\s*(?:(?:public|protected|private)\s+)?"
    r"(?:(?:static|final|native|synchronized|abstract|strictfp|default)\s+)*"
    r"(?:<[^>{}]+>\s*)?"
    r"(?:[A-Za-z_$][\w$<>,.?\[\]@]*\s+)+"
    r"[A-Za-z_$][\w$]*\s*\([^;{}]*\)\s*(?:throws\s+[^{}]+)?\{\s*$"
)

CTOR_RE = re.compile(
    r"^\s*(?:(?:public|protected|private)\s+)?"
    r"[A-Za-z_$][\w$]*\s*\([^;{}]*\)\s*(?:throws\s+[^{}]+)?\{\s*$"
)


@dataclass
class LineParseState:
    in_block_comment: bool = False


@dataclass
class BraceState:
    in_block_comment: bool = False
    in_string: bool = False
    in_char: bool = False
    escape: bool = False


@dataclass
class MethodBlock:
    file: Path
    start_line: int
    end_line: int
    start_brace_col: int
    end_brace_col: int
    signature: str
    exec_lines: int


def strip_line_comments(line: str, state: LineParseState) -> str:
    out: list[str] = []
    i = 0
    in_string = False
    in_char = False
    escape = False

    while i < len(line):
        ch = line[i]
        nxt = line[i + 1] if i + 1 < len(line) else ""

        if state.in_block_comment:
            if ch == "*" and nxt == "/":
                state.in_block_comment = False
                i += 2
            else:
                i += 1
            continue

        if in_string:
            out.append(ch)
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                in_string = False
            i += 1
            continue

        if in_char:
            out.append(ch)
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == "'":
                in_char = False
            i += 1
            continue

        if ch == "/" and nxt == "*":
            state.in_block_comment = True
            i += 2
            continue

        if ch == "/" and nxt == "/":
            break

        out.append(ch)
        if ch == '"':
            in_string = True
        elif ch == "'":
            in_char = True
        i += 1

    return "".join(out)


def brace_delta(line: str, state: BraceState) -> tuple[int, list[int], list[int]]:
    delta = 0
    open_cols: list[int] = []
    close_cols: list[int] = []
    i = 0

    while i < len(line):
        ch = line[i]
        nxt = line[i + 1] if i + 1 < len(line) else ""

        if state.in_block_comment:
            if ch == "*" and nxt == "/":
                state.in_block_comment = False
                i += 2
            else:
                i += 1
            continue

        if state.in_string:
            if state.escape:
                state.escape = False
            elif ch == "\\":
                state.escape = True
            elif ch == '"':
                state.in_string = False
            i += 1
            continue

        if state.in_char:
            if state.escape:
                state.escape = False
            elif ch == "\\":
                state.escape = True
            elif ch == "'":
                state.in_char = False
            i += 1
            continue

        if ch == "/" and nxt == "*":
            state.in_block_comment = True
            i += 2
            continue

        if ch == "/" and nxt == "/":
            break

        if ch == '"':
            state.in_string = True
            i += 1
            continue

        if ch == "'":
            state.in_char = True
            i += 1
            continue

        if ch == "{":
            delta += 1
            open_cols.append(i)
        elif ch == "}":
            delta -= 1
            close_cols.append(i)

        i += 1

    return delta, open_cols, close_cols


def is_brace_only(line: str) -> bool:
    compact = "".join(line.split())
    compact = compact.rstrip(";")
    return bool(compact) and set(compact).issubset({"{", "}"})


def is_method_start(line: str) -> bool:
    if "(" not in line or ")" not in line or "{" not in line:
        return False
    stripped = line.strip()
    if not stripped.endswith("{"):
        return False

    lower = stripped.lower()
    if "->" in stripped:
        return False
    if any(token in lower for token in (" class ", " interface ", " enum ", " record ")):
        return False

    first_token = stripped.split(None, 1)[0]
    if first_token in CONTROL_KEYWORDS:
        return False

    before_paren = stripped.split("(", 1)[0]
    if "." in before_paren:
        return False

    if METHOD_RE.match(line):
        return True
    if CTOR_RE.match(line):
        return True
    return False


def count_exec_lines(lines: list[str], start_line: int, end_line: int, start_col: int, end_col: int) -> int:
    state = LineParseState()
    count = 0

    for idx in range(start_line, end_line + 1):
        raw = lines[idx]

        if idx == start_line:
            segment = raw[start_col + 1 :]
        elif idx == end_line:
            segment = raw[:end_col]
        else:
            segment = raw

        cleaned = strip_line_comments(segment, state).strip()
        if not cleaned:
            continue
        if is_brace_only(cleaned):
            continue
        count += 1

    return count


def collect_methods(file_path: Path) -> list[MethodBlock]:
    lines = file_path.read_text(encoding="utf-8").splitlines()
    methods: list[MethodBlock] = []
    brace_state = BraceState()

    i = 0
    while i < len(lines):
        line = lines[i]
        if not is_method_start(line):
            i += 1
            continue

        _, open_cols, _ = brace_delta(line, BraceState())
        if not open_cols:
            i += 1
            continue

        start_col = open_cols[-1]
        depth = 1
        cursor = i
        end_col = -1
        local_state = BraceState()

        # Consume from opening line first.
        line_delta, line_opens, line_closes = brace_delta(lines[cursor], local_state)
        if line_opens:
            # Ignore opens before declaration open brace by zeroing those positions.
            for col in line_opens:
                if col < start_col:
                    depth -= 1
        depth = 0
        seen_start = False

        while cursor < len(lines):
            current = lines[cursor]
            delta, opens, closes = brace_delta(current, brace_state)

            if cursor == i:
                # Start counting braces from the declaration brace position.
                for col in opens:
                    if col >= start_col:
                        depth += 1
                        seen_start = True
                for col in closes:
                    if seen_start:
                        depth -= 1
                        if depth == 0:
                            end_col = col
                            break
            else:
                for col in opens:
                    if seen_start:
                        depth += 1
                for col in closes:
                    if seen_start:
                        depth -= 1
                        if depth == 0:
                            end_col = col
                            break

            if seen_start and depth == 0:
                break
            cursor += 1

        if end_col < 0:
            i += 1
            continue

        exec_lines = count_exec_lines(lines, i, cursor, start_col, end_col)
        methods.append(
            MethodBlock(
                file=file_path,
                start_line=i + 1,
                end_line=cursor + 1,
                start_brace_col=start_col,
                end_brace_col=end_col,
                signature=lines[i].rstrip(),
                exec_lines=exec_lines,
            )
        )
        i = cursor + 1

    return methods


def main() -> int:
    violations: list[MethodBlock] = []

    files: list[Path] = []
    for directory in TARGET_DIRS:
        if directory.exists():
            files.extend(sorted(directory.rglob("*.java")))

    for file_path in files:
        for method in collect_methods(file_path):
            if method.exec_lines > LIMIT:
                violations.append(method)

    if not violations:
        print(f"[OK] No methods exceed {LIMIT} execution lines.")
        return 0

    print(f"[FAIL] Found {len(violations)} methods exceeding {LIMIT} execution lines:")
    for item in sorted(violations, key=lambda x: (str(x.file), x.start_line)):
        relative = item.file.as_posix()
        print(f" - {relative}:{item.start_line} ({item.exec_lines} lines)")

    return 1


if __name__ == "__main__":
    sys.exit(main())
PY

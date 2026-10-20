#!/usr/bin/env python3
"""Check that every function import of the engine's wasm has a host function.

The engine's wasm module is the contract between the game and this host. An
import without a host implementation ends the boot: the generated glue calls
pg_missing_import and the engine traps. This script lists what is still
missing, so the gap is visible in CI instead of on the phone.

  python3 check_imports.py <module.wasm> [--root app]

Exit code 1 when an import has no implementation.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from wasm_imports import function_imports  # noqa: E402


def c_name(module: str, field: str) -> str:
    """The symbol wasm2c expects for an import."""
    if module == "env":
        prefix = "w2c_env_"
    elif module == "wasi_snapshot_preview1":
        prefix = "w2c_wasi__snapshot__preview1_"
    else:
        return ""
    # wasm2c mangles a leading underscore to 0x5F.
    if field.startswith("_"):
        field = "0x5F" + field[1:]
    return prefix + field


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("wasm")
    parser.add_argument("--root", default=None, help="the app/ directory (default: next to Scripts/)")
    options = parser.parse_args()

    root = Path(options.root) if options.root else Path(__file__).resolve().parents[1]
    sources = [p for p in (root / "Source").glob("*") if p.suffix in (".c", ".h", ".mm")]
    generated = root / "Generated" / "Imports.c"
    if generated.is_file():
        sources.append(generated)
    text = "\n".join(p.read_text(encoding="utf-8", errors="replace") for p in sources)
    defined = set(re.findall(r"\b(w2c_(?:env|wasi__snapshot__preview1)_[A-Za-z0-9_]+)\s*\(", text))

    data = Path(options.wasm).read_bytes()
    missing = []
    total = 0
    for module, field in function_imports(data):
        total += 1
        symbol = c_name(module, field)
        if symbol and symbol in defined:
            continue
        missing.append((module, field, symbol))

    print(f"{total} function imports, {total - len(missing)} implemented, {len(missing)} missing")
    for module, field, symbol in missing:
        print(f"  MISSING {module}.{field} -> {symbol}")
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""List the function imports of a WebAssembly module.

Used by check_imports.py: the engine's wasm module is the contract between the
game and this host, and every import in it needs a host function. The parser
reads only the type and import sections, so it is fast on a 63 MB module.

Usage: python3 wasm_imports.py <module.wasm>
"""
from __future__ import annotations

import sys
from pathlib import Path


def uleb(buf: bytes, i: int) -> tuple[int, int]:
    result = 0
    shift = 0
    while True:
        byte = buf[i]
        i += 1
        result |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return result, i
        shift += 7


def name(buf: bytes, i: int) -> tuple[str, int]:
    length, i = uleb(buf, i)
    return buf[i:i + length].decode("utf-8", "replace"), i + length


def function_imports(data: bytes) -> list[tuple[str, str]]:
    """Return [(module, field)] for every function import."""
    types: list[tuple[list[int], list[int]]] = []
    imports: list[tuple[str, str, int]] = []
    i = 8
    while i < len(data):
        section = data[i]
        i += 1
        size, i = uleb(data, i)
        end = i + size
        if section == 1:  # type
            j = i
            count, j = uleb(data, j)
            for _ in range(count):
                form = data[j]
                j += 1
                if form != 0x60:
                    continue
                params, j = uleb(data, j)
                param_types = list(data[j:j + params])
                j += params
                results, j = uleb(data, j)
                result_types = list(data[j:j + results])
                j += results
                types.append((param_types, result_types))
        elif section == 2:  # import
            j = i
            count, j = uleb(data, j)
            for _ in range(count):
                module, j = name(data, j)
                field, j = name(data, j)
                kind = data[j]
                j += 1
                if kind == 0:  # function
                    type_index, j = uleb(data, j)
                    imports.append((module, field, type_index))
                elif kind == 1:  # table
                    j += 1
                    limit = data[j]
                    j += 1
                    _, j = uleb(data, j)
                    if limit:
                        _, j = uleb(data, j)
                elif kind == 2:  # memory
                    limit = data[j]
                    j += 1
                    _, j = uleb(data, j)
                    if limit:
                        _, j = uleb(data, j)
                elif kind == 3:  # global
                    j += 2
        i = end
    return [(module, field) for module, field, _ in imports]


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    data = Path(sys.argv[1]).read_bytes()
    for module, field in function_imports(data):
        print(f"{module}.{field}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

"""Locate the wasm2c output file that defines a given function.

wasm2c splits the module across game_*.c, and which file a function lands in
depends on --num-outputs. Upstream's patch scripts hardcode game_0.c/game_15.c
because the split is fixed at 64; this port raises the count for build speed, so
the containing file has to be looked up rather than assumed.
"""
from pathlib import Path


def defining_file(root: Path, signature: str) -> Path:
    """Return the game_*.c that declares `signature`.

    Raise RuntimeError rather than returning None: a function that has moved or
    vanished means the pinned wasm changed, and silently skipping the patch would
    produce an engine that boots but misbehaves.
    """
    generated = root / "Generated"
    for path in sorted(generated.glob("game_*.c")):
        if signature in path.read_text(encoding="utf-8", errors="replace"):
            return path
    raise RuntimeError(
        f"No game_*.c declares {signature!r}. The pinned wasm no longer matches "
        f"the function bodies these patch scripts match by name."
    )
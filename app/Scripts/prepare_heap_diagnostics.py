"""Add fault details to the translation unit containing the heap allocator."""
from pathlib import Path

root = Path(__file__).resolve().parents[1]
source = (root / 'Generated/game-impl.h').read_text()
old = 'if (UNLIKELY(res > (mem)->size))             \\\n      TRAP(OOB);'
new = 'if (UNLIKELY(res > (mem)->size)) {           \\\n      pg_record_memory_fault(offset, len, (mem)->size); \\\n      TRAP(OOB); }'
if old not in source:
    raise RuntimeError('Pinned wasm2c range-check diagnostic patch no longer matches.')
source = 'extern void pg_record_memory_fault(unsigned long long, unsigned long long, unsigned long long);\n' + source.replace(old, new)
target = root / 'Generated/game-heap-impl.h'
if not target.exists() or target.read_text() != source:
    target.write_text(source)
# game-heap-impl.h carries the patched helper definitions, and it has to be
# included by exactly one translation unit. Upstream hardcodes game_63.c, the
# last of wasm2c's 64 outputs. Refining --num-outputs splits the same functions
# more finely but does not reorder them, so "the last output" is still the right
# unit; pick it by number rather than by a count baked into this script.
unit = max((root / 'Generated').glob('game_*.c'),
           key=lambda p: int(p.stem.split('_')[1]))
text = unit.read_text().replace('#include "game-impl.h"', '#include "game-heap-impl.h"')
if unit.read_text() != text:
    unit.write_text(text)

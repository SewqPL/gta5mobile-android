#!/usr/bin/env python3
"""Prepare the generated engine sources for an Android build from your game files.

This is the Linux/NDK counterpart of the upstream app/Scripts/prepare.py. It runs
the same patched-wasm2c pipeline to turn b/<build>/game.wasm into the C sources
(Generated/game_*.c + game.h) that the vendored engine core compiles against.

Differences from the iOS script, all deliberate:
  - no Apple-Silicon check, no xcrun/xcodebuild, no Metal shader step (the Vulkan
    renderer does not exist yet, so there is nothing to precompile shaders into)
  - wabt and simde are fetched for the host platform
  - output is a CMake source list consumed by android/app/CMakeLists.txt

It reuses the upstream patch scripts verbatim so the generated code matches the
pinned engine exactly. Run on Linux (CI) with the game mirror available.

Usage: python3 prepare_android.py --mirror /path/to/game-folder
"""
from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import urllib.request

# This script lives in app/Scripts, so parents[1] is app/. That is exactly the
# upstream staging root: the vendored helper scripts (generate_imports, the
# prepare_* family) resolve their own paths as parents[1] and expect
# <root>/Tools, <root>/Generated and <root>/Source. Staging here keeps those
# scripts byte-for-byte unmodified.
ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / "Scripts"
UPSTREAM = ROOT.parent / "vendor" / "fivestar"

# The upstream pipeline pins this exact engine build; the virtual-pad and
# unchecked-memory patches match generated function bodies by name and offset.
PINNED_WASM_SHA256 = "11ca8d2c04c5e843d18ff4aea4899d72c86973c6b031df334e67c446b2ae83e0"
BUILD_ID = "8b0b5899ed"


def run(*args: list[str], cwd: Path = ROOT) -> None:
    subprocess.run([str(a) for a in args], cwd=cwd, check=True)


def download(url: str, destination: Path) -> None:
    with urllib.request.urlopen(url) as response:
        destination.write_bytes(response.read())


def find_host_compiler() -> str:
    """Locate a C++ compiler to build the host wasm2c.

    Honours HOST_CLANGXX, then falls back to clang++ / clang on PATH. wasm2c is
    C++20, so a plain C compiler will not do; fail early with a clear message.
    """
    override = os.environ.get("HOST_CLANGXX")
    if override:
        return override
    for candidate in ("clang++", "clang++-18", "clang++-17", "g++"):
        if shutil.which(candidate):
            return candidate
    raise SystemExit(
        "No C++ compiler found to build the host wasm2c. Install clang or set "
        "HOST_CLANGXX to a C++20-capable compiler (e.g. 'clang++')."
    )


def safe_extract(archive: Path, destination: Path) -> None:
    from safe_extract import extract  # upstream helper, keeps tar traversal safe
    with tarfile.open(archive) as package:
        extract(package, destination)


def host_wabt_asset() -> str:
    """Map this host to a wabt 1.0.42 prebuilt asset.

    wabt publishes -x64 and -arm64 variants; the names are explicit, there is no
    bare "linux" tarball. Verified against the 1.0.42 release assets.
    """
    machine = platform.machine().lower()
    system = platform.system()
    if system == "Linux":
        return "wabt-1.0.42-linux-arm64.tar.gz" if machine in ("arm64", "aarch64") else "wabt-1.0.42-linux-x64.tar.gz"
    if system == "Darwin":
        # wabt ships macOS arm64 only.
        return "wabt-1.0.42-macos-arm64.tar.gz"
    if system == "Windows":
        return "wabt-1.0.42-windows-x64.tar.gz"
    raise SystemExit(f"No prebuilt wabt 1.0.42 for {system}/{machine}; build wabt from source into Tools/wabt-1.0.42.")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mirror", type=Path, required=True, help="folder with data/ and b/<build>/game.wasm")
    options = parser.parse_args()

    engine = options.mirror / f"b/{BUILD_ID}/game.wasm"
    if not engine.is_file():
        raise SystemExit(f"Missing {engine}. The engine core is pinned to build {BUILD_ID}.")
    digest = hashlib.sha256(engine.read_bytes()).hexdigest()
    if digest != PINNED_WASM_SHA256:
        raise SystemExit(
            "The engine does not match the build this core supports.\n"
            f"  expected {PINNED_WASM_SHA256}\n  found    {digest}\n"
            "Update PINNED_WASM_SHA256 and re-check the generated-code patches "
            "(prepare_virtual_pad.py matches ioPad::Update by body) before forcing it."
        )

    for name in ("Tools", "Generated"):
        (ROOT / name).mkdir(exist_ok=True)

    # generate_imports.py globs <root>/Source to decide which wasm imports are
    # already implemented by the host, so it must see the host C sources under the
    # upstream name. Mirror the vendored files (the .m/.mm Metal/Foundation sources
    # are not part of the Android core and are intentionally absent).
    source_stage = ROOT / "Source"
    source_stage.mkdir(exist_ok=True)
    for vendored in UPSTREAM.glob("*.c"):
        shutil.copyfile(vendored, source_stage / vendored.name)
    for vendored in UPSTREAM.glob("*.h"):
        shutil.copyfile(vendored, source_stage / vendored.name)
    tools = ROOT / "Tools"
    generated = ROOT / "Generated"

    # 1. wabt (provides libwabt.a, headers and the wasm2c runtime sources)
    wabt = tools / "wabt-1.0.42"
    if not wabt.exists():
        release = "https://github.com/WebAssembly/wabt/releases/download/1.0.42/"
        filename = host_wabt_asset()
        archive = tools / "wabt.tar.gz"
        download(release + filename, archive)
        checksum = urllib.request.urlopen(release + filename + ".sha256").read().decode().split()[0]
        if hashlib.sha256(archive.read_bytes()).hexdigest() != checksum:
            raise SystemExit("WABT archive checksum mismatch.")
        safe_extract(archive, tools)
        if not wabt.exists():
            # The tarball extracts to wabt-1.0.42; normalize if it differs.
            for candidate in tools.glob("wabt-*"):
                if candidate.is_dir():
                    candidate.rename(wabt)
                    break

    # 2. simde (the wasm2c output uses x86 SIMD intrinsics emulated on arm64)
    simde = tools / "simde"
    if not simde.exists():
        run("git", "clone", "--depth", "1", "--branch", "v0.8.2", "https://github.com/simd-everywhere/simde.git", simde)

    # 3. upstream wasm2c sources + the atomic-wait/notify c-writer patch
    for upstream, local in (("src/c-writer.cc", "c-writer-original.cc"), ("src/tools/wasm2c.cc", "wasm2c-main.cc")):
        destination = tools / local
        if not destination.exists():
            download("https://raw.githubusercontent.com/WebAssembly/wabt/1.0.42/" + upstream, destination)
    run(sys.executable, str(SCRIPTS / "patch_wasm2c.py"))

    # 4. Build the host wasm2c with the patched c-writer.
    #    This is a HOST compiler, not the NDK cross compiler: wasm2c is a
    #    build-time tool that emits C.
    #
    #    The upstream macOS build links a sha256_shim.c because the prebuilt
    #    macOS libwabt.a references OpenSSL's SHA256. The Linux and Windows
    #    prebuilts have no such dangling reference, so no shim is needed.
    clangxx = find_host_compiler()
    output = tools / ("wasm2c-native.exe" if os.name == "nt" else "wasm2c-native")
    run(clangxx, "-std=c++20", "-O1", "-I", str(wabt / "include"),
        str(tools / "wasm2c-main.cc"), str(tools / "c-writer-patched.cc"),
        str(wabt / "lib/libwabt.a"), "-o", str(output))

    # 5. wasm2c the engine into C (64 output files, like upstream).
    run(str(output), "--enable-threads", "--no-debug-names",
        "--module-name=game", "--num-outputs=64", str(engine), "-o", str(generated / "game.c"))

    # 6. wasm-rt host patches: trap hook (EngineHost.c supplies pg_record_trap).
    for rt in ("wasm-rt-impl.c", "wasm-rt-mem-impl.c", "wasm-rt-exceptions-impl.c"):
        src = wabt / "share/wabt/wasm2c" / rt
        dst = generated / rt
        shutil.copyfile(src, dst)
    impl = generated / "wasm-rt-impl.c"
    text = impl.read_text()
    if "pg_record_trap" not in text:
        text = text.replace(
            "void wasm_rt_trap(wasm_rt_trap_t code) {",
            "extern void pg_record_trap(int) __attribute__((weak));\n"
            "void wasm_rt_trap(wasm_rt_trap_t code) {\n  if(pg_record_trap)pg_record_trap(code);",
        )
        impl.write_text(text)

    # 7. Upstream generated-code patches (imports, heap diagnostics, memory
    #    pressure, virtual pad, unchecked memory). Run in upstream order.
    run(sys.executable, str(SCRIPTS / "generate_imports.py"))
    run(sys.executable, str(SCRIPTS / "prepare_heap_diagnostics.py"))
    run(sys.executable, str(SCRIPTS / "prepare_memory_pressure.py"))
    run(sys.executable, str(SCRIPTS / "prepare_virtual_pad.py"))
    run(sys.executable, str(SCRIPTS / "prepare_unchecked_memory.py"))

    # 8. Emit the CMake source list the NDK build consumes.
    #    game_*.c is the engine; Imports.c holds the typed wasm import bindings
    #    generate_imports.py emits for anything the host does not implement; the
    #    wasm-rt-*.c files are wabt's runtime (shared memory, exceptions, traps).
    #    All four groups are required to link, so list them together.
    sources = sorted(generated.glob("game_*.c"))
    if not sources:
        raise SystemExit("wasm2c produced no game_*.c files.")
    runtime = [generated / name for name in
               ("wasm-rt-impl.c", "wasm-rt-mem-impl.c", "wasm-rt-exceptions-impl.c")]
    missing = [p.name for p in runtime if not p.is_file()]
    if missing:
        raise SystemExit("Missing wabt runtime sources: " + ", ".join(missing))
    imports = generated / "Imports.c"
    if not imports.is_file():
        raise SystemExit("Missing Imports.c (generate_imports.py did not run).")

    listing = generated / "game_sources.cmake"
    listing.write_text(
        "# Generated by prepare_android.py. Do not edit.\n"
        "set(GAME_CODE_SOURCES\n"
        + "".join(f"  {p.as_posix()}\n" for p in sources + runtime + [imports])
        + ")\n"
    )
    print(f"Prepared {len(sources)} engine source files plus runtime and imports in {generated}")
    print(f"CMake source list: {listing}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
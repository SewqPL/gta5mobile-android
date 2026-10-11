# GTA V Android port — vendored engine core

This directory holds the Android port of the engine core from
[gta5-ios-port](https://github.com/kevinerikjs/gta5-ios-port) (the "5★ Port"
project, MIT licensed for its own code). We keep this project's Android shell
(the download/installer service, launcher, touch HUD, controller compat) and
replace only the engine runtime: instead of interpreting `game.wasm` with
Wasmtime, we run the wasm2c-compiled engine as native ARM64 C, the same way the
iOS app does.

Game files (`game.wasm`, `data/`, shaders) are yours and never committed.

## What is here

- `vendor/fivestar/` — the engine host sources copied from the upstream
  `app/Source/` (the 6 `.c` files and their headers). Only the Metal renderer
  (`.mm`) and the Foundation asset streamer (`.m`) are left behind; they are the
  parts being replaced for Android.
- `vendor/fivestar/PGPort.h` — the Android substitutions for the three Apple
  APIs the host uses (`TARGET_OS_SIMULATOR`, `os_proc_available_memory`, and the
  dyld image base for trap logs).
- `vendor/fivestar/PGGraphicsAndroid.h` + `PGGraphicsHeadless.c` — the graphics
  seam. On iOS the Swift layer hands the renderer a `CAMetalLayer`; on Android it
  hands the Vulkan renderer an `ANativeWindow`. Until the Vulkan renderer lands,
  `PGGraphicsHeadless.c` provides a stub that accepts the command ring and does
  not render.
- `vendor/fivestar/EngineBridge.c` — the JNI bridge. It is the only file that
  knows about both JNI and `EngineHost.h`. It exposes the engine to the Java
  shell: start/stop, attach the `ANativeWindow`, push controller state, pull
  audio, and read status (pause menu, last input device, touch context, FPS).
  It also translates Muguet's keyboard/touch input into the engine's analog
  `PGPadState`, so GTA's own controller code (assisted aim, analog driving)
  runs as intended.
- `android/app/src/main/java/dev/muguet/EngineActivity.java` — hosts the engine.
  A plain `SurfaceView` (not `NativeActivity`/winit), so we own the `ANativeWindow`
  for the future Vulkan renderer and the touch HUD is a normal overlay — no
  sub-window hack. Runs a 60 Hz input pump and an `AudioTrack` pull thread.
- `app/Scripts/prepare_android.py` — the wasm2c pipeline. Downloads wabt 1.0.42
  and simde, builds the upstream-patched wasm2c, turns your `game.wasm` into the
  `Generated/game_*.c` C sources, applies the upstream generated-code patches
  (imports, heap diagnostics, memory pressure, virtual pad, unchecked memory),
  and writes a CMake source list.
- `app/Scripts/{generate_imports,prepare_*}.py` — the upstream patch scripts,
  used verbatim.
- `android/app/CMakeLists.txt` — builds the engine core + JNI bridge
  (`libgta5engine.so`) for Android arm64, reproducing the upstream iOS defines.

## Two Android runtimes, side by side

The launcher now offers two Play buttons:

- **Play (wasmtime engine)** → `GameActivity`, the original Rust/Wasmtime path
  (`libmuguet.so`). Unchanged, still the default CI artifact.
- **Play (native wasm2c engine)** → `EngineActivity`, the vendored wasm2c engine
  (`libgta5engine.so`). Requires the generated game code.

Both read the same `config.json` and the same installed game folder, so the
downloader/installer is shared.

## The hard gate: game files

The engine host is generated code. Every host translation unit includes
`HostInternal.h` → the wasm2c-generated `game.h`. **Nothing in this port
compiles until you run the wasm2c pipeline against your own game file**, and the
pipeline is pinned to one specific build:

- build id `8b0b5899ed`
- `game.wasm` sha256 `11ca8d2c04c5e843d18ff4aea4899d72c86973c6b031df334e67c446b2ae83e0`

`prepare_android.py` verifies the hash and refuses anything else, because the
generated-code patches (notably `prepare_virtual_pad.py`, which hooks
`ioPad::Update`) match pinned function bodies. If your web-port files are a
different build, you must re-pin the patches by hand — do not force it.

To generate the code (on Linux; CI or a dev box with the NDK):

```sh
python3 app/Scripts/prepare_android.py --mirror /path/to/game-folder
```

That folder must contain `data/` and `b/8b0b5899ed/game.wasm`.

## Build and package

The existing Android app still ships `libmuguet.so` (the Rust/Wasmtime path) so
the normal build and CI stay green. To additionally build and package the engine
core:

```sh
gradle -p android assembleDebug -PwithEngineCore=true
```

`-PwithEngineCore=true` adds the CMake `externalNativeBuild`, which requires the
generated game code to exist (see above). The result is an APK containing both
`libmuguet.so` and `libgta5engine.so`.

## What is done, and what is not

Done:
- Engine host sources vendored and compiling against the upstream iOS defines.
- The three Apple-only dependencies neutralized in `PGPort.h`.
- The graphics seam isolated behind four functions, so nothing above it changes
  when the renderer arrives.
- wasm2c pipeline ported for Linux/NDK.

Not done (the renderer):
- `MRRenderer.mm` (the ~99 KB Metal renderer that interprets the D3D11 command
  ring) has **not** been ported. Android needs a Vulkan renderer that consumes
  the same command ring. That is the bulk of the remaining work and is the reason
  the port currently builds but does not draw.
- Shader precompilation: the iOS build bakes MSL via Tint. Android needs SPIR-V.
  Nothing is precompiled yet; the headless stub does not need it.
- `NativeAssets.m` (the Foundation streaming asset reader with the packed 4 KB
  block cache) is excluded. The Android app already downloads game files ahead of
  time via `GameDownloadService`, so on-device streaming is not yet needed; a
  local-files asset path will replace it.

## Licensing and assets

The vendored host code is MIT (from the upstream project). It contains no
Rockstar/Take-Two code or data. The game files are yours, fetched at build time,
and never stored in this repository.
# BytArch GTA-V

A native port of the GTA V web build. The game ships as WebAssembly; this throws
the browser layer away and runs the same `game.wasm` as an ordinary native app —
[wasmtime](https://wasmtime.dev) for the code, [wgpu](https://wgpu.rs) on Vulkan
for Android and Metal on Apple platforms.

Unofficial. No game content is included: you bring your own copy of the web
port's data.

- **Android (working target)** — ARM64, Android 10+, Vulkan. See below.
- **iOS / macOS** — Metal, JIT on macOS, AOT on iOS. Build scripts at the bottom.

## Android

**Download:** [BytArch GTA-V for Android 0.1.11-patch](https://github.com/bytarch/gta5mobile-android/releases/tag/android-0.1.11-patch)
- `BytArch-GTA-V-android-0.1.11-patch-debug.apk`, arm64-v8a, Android 10+, debug
  signed (sideloading only). Sideload it, or `adb install -r` over an older copy.

Push to `main` and GitHub Actions builds an APK; the `Muguet-Android-arm64-debug`
artifact appears on the completed run. Locally: `sh make-android.sh` or
`.\make-android.ps1`, output at
`android/app/build/outputs/apk/debug/app-debug.apk`.

### Game data

The launcher never downloads game files. Point it at data you already have, in
either form — the layout is the folder holding `data/` and `b/<build>/`:

```
data/manifest.json
b/<build>/shaders/index.json
b/<build>/game.wasm        (only for the native wasm2c engine)
```

- **Use a game files folder** — pick a folder that already holds `data/` and
  `b/`. It is used where it lies, so nothing is copied and no free space is
  needed. The game folder does not have to be the picked one: the launcher
  looks inside what you picked, so the release folder (`GrandV-assets`, which
  holds `mirror/playgta5.com/data/` and `.../b/`) works as it is.
- **Import game ZIP** — pick a `.zip` of the same tree. It is extracted into app
  storage, which needs free space for the archive and the extracted data alike.
  A wrapper folder inside the archive is stripped, so the release ZIP works too.

The folder must be on internal storage (`/storage/emulated/0/...`). Android
hands an app a folder permission but still refuses to `open()` files another app
owns, which is exactly what data copied over by a downloader or torrent app
looks like — so the launcher offers **Grant file access** ("All files access")
when that happens.

### Graphics

Every graphics knob the web launcher exposes is in the launcher, under
**Graphics**, with the same names, plus Low / Engine default / Max presets.
They are sent to the engine as arguments rather than written into the save file,
because the engine rewrites its own settings on every save.

Texture, reflection, water, particle and grass quality; scenery, pedestrian and
vehicle detail; city density; variety; post FX; motion blur; depth of field;
shadow quality and softness; ambient occlusion; render distance; fog volumes;
particle and long shadows; loading ahead; distant lights.

For Wasmtime black-screen reports, two independent switches are available
under Graphics:

- **Force software clip distances** emulates clipping in the fragment stage.
  Adreno devices get this automatically: their Vulkan driver loses the device
  on hardware clip distances, which is what left the world black.
- **Decode BC textures on CPU** tests the existing decompression fallback even
  when the GPU advertises native BC support. This uses more memory.

Export logs for each run. The logs record the selected diagnostics, GPU driver,
pipeline creation failures and device-loss reason.

**Low (phone default)** turns shadows off entirely, pins detail to the floor and
pulls the render distance to 500 m. Measured on a Dimensity 8000, pinning LOD
alone cut draw calls from 2082 to 579 per frame; shadow and reflection passes
account for most of the rest.

### Multiplayer

Multiplayer uses the Grand-V relay — the same transport as the web version, so
a room can be shared with browser players. In the launcher: enable
**Multiplayer**, then fill in the relay address (`ws://host:8012/relay`), the
room name, and the access code from whoever is hosting. Plain `ws://` for now, no
TLS.

The relay carries pose only: position, rotation and model at 20 Hz. Rooms hold
two players. That is the whole protocol — see `src/relay.rs` for the client and
[bytarch/Grand-V-Android-Port](https://github.com/bytarch/Grand-V-Android-Port)
for the server.

### Native engine core

A separate build path compiles the upstream
[gta5-ios-port](https://github.com/kevinerikjs/gta5-ios-port) engine core with
wasm2c, running `game.wasm` as native ARM64 C instead of interpreting it. It is
not the default build, and it is still the rougher of the two.

Graphics go through the same wgpu renderer as the wasmtime path: the engine
core's graphics worker hands its command ring to `muguet_gpu_start` in the Rust
library, and the engine's own futex (wait on the ring head, wake the producer)
is registered with the renderer, so the ring protocol is the one the browser's
wgpu worker speaks. The renderer is resolved with `dlsym`, so the engine core
never links against it and a build without the renderer still boots (headless).

## Building for Apple platforms

macOS:

```
./make-app.sh
```

gives `build/Muguet.app`.

iOS:

```
TEAM=<your team id> ./make-ios.sh game.zip --install
```

`--ipa` produces `build/Muguet.ipa`. With a free account add `FREE=1` plus a
`BUNDLE_ID` of your own. Install through AltStore; Sideloadly and SideStore are
known to be problematic.

### Requirements

Apple Silicon Mac, macOS 14+, Rust toolchain, Xcode 26+. For iOS also
`rustup toolchain install 1.98.1`, `rustup target add aarch64-apple-ios
--toolchain 1.98.1`, `xcodegen` (`brew install xcodegen`), an Apple developer
account, and a device on iOS 15+ with ideally 8 GB of RAM.

## Limitations

- ARM64 only on Android, Vulkan required, Android 10+.
- First launch compiles the game and takes several minutes; early stutter is
  mostly loading from storage.
- The native wasm2c engine renders through the same wgpu renderer, but it is
  pinned to game build `8b0b5899ed` (wasm sha256 `11ca8d2c…`): a newer web
  build of the game needs the wasm2c pins re-checked first.
- Multiplayer is two-player pose replication over a relay you do not host.
- iOS needs the increased memory limit entitlement; the minimap does not show;
  story missions are single player only.

## Repository layout

```
src/            the port itself: engine host, graphics, audio, input, netcode
android/        Gradle app, the Java launcher UI, NDK build
app/            native wasm2c engine core (work in progress)
ios/, launcher/ iOS and macOS shells
vendor/         patched wgpu-core and the wasm2c bridge
```

## Reporting bugs

- Issue tracker: https://github.com/bytarch/gta5mobile-android/issues
- Direct contact: https://x.com/MikeJohndlvl

The launcher has both links at the bottom of its screen. Include your phone
model, Android version and Vulkan driver, plus the exported logs from the
`Export logs` button when the crash is not reproducible.

## Legal

This repository contains no game assets. Users supply their own copy of the
web port's data, and this project neither hosts nor distributes it.

Contributions welcome.

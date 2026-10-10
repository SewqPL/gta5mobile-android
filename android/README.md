# Muguet Android — experimental

This adds an **ARM64 native Android application**, not an iOS emulator or a
WebView. It uses Wasmtime for `game.wasm`, wgpu/Vulkan for graphics, CPAL/AAudio
for audio, Android views for touch controls, and the existing UDP multiplayer.
No proprietary game files are included in the APK.

**Not yet verified in-game.** Building and testing with an actual device and
compatible game files is required before calling this a working port.

## Build with GitHub Actions

Push to `main` or run **Actions → Android APK → Run workflow**. The workflow
installs the SDK/NDK, Rust, JDK 17 and Gradle, builds the ARM64 APK, and uploads
**Muguet-Android-arm64-debug** in the run's **Artifacts** section. Download and
extract it to get `app-debug.apk`. A separate job runs Rust unit tests.
The first build may be slow. Artifacts expire after 14 days; rebuild as needed.
No game ZIP, signing credentials, or game-code cache belongs in the repository.

## Local build prerequisites

- Current stable Rust supporting the existing Wasmtime 49 dependencies.
- JDK **17** (set `JAVA_HOME` accordingly) and **Gradle 8.13** on `PATH`.
- Android SDK with `platforms;android-36`, `build-tools;36.0.0`, and
  `ndk;28.2.13676358` installed from Android Studio's SDK Manager.
- `ANDROID_HOME` pointing to the SDK, or `android/local.properties` with `sdk.dir`.
- `cargo install cargo-ndk --version 4.1.2 --locked`.

Windows / PowerShell:

```powershell
.\make-android.ps1
# With a USB-debugging device connected:
.\make-android.ps1 -Install
```

Linux / macOS:

```sh
sh make-android.sh
sh make-android.sh --install
```

Output: `android/app/build/outputs/apk/debug/app-debug.apk` (debug signed).
The build downloads public Cargo/Gradle dependencies, builds `libmuguet.so` for
`arm64-v8a`, and packages it. Native library alignment is set to 16 KiB; this
still needs verification on a 16 KiB-page Android device. Release distribution
requires your own signing configuration; never publish the debug key.
Native compilation targets API 29 (the app's minimum Android version), separately
from Java's compile/target SDK 36. NDK r28 supports native APIs through 35; the
Gradle build checks that the requested native API is supported before compiling.

## On the device

1. Use Android 10+ on an ARM64 device with Vulkan 1.1. A high-end phone with
   **8–12 GB RAM** is recommended, but does not guarantee compatibility.
2. Open Muguet, select **Import game ZIP**, and pick your own compatible
   web-port archive: `data/manifest.json`, `b/<build>/game.wasm`, and
   `b/<build>/shaders/index.json`. A regular Steam/PC installation alone is
   not compatible with this runtime.
3. Keep the launcher open while importing. Allow space for a temporary copy
   of the ZIP plus the extracted files (and the old installation if replacing
   it). Imported data, saves and config live in app-private storage; uninstalling
   the app deletes them. Import cannot run as a background service.
4. Start at **30 FPS, 0.5 render scale, 5 GB maximum heap**. The heap cap is
   not total app memory: textures, compilation and native workers need more.
5. First Play patches and compiles the Wasm on a worker thread and caches the
   generated native code. This can take several minutes and consume substantial
   RAM. Later launches reuse the cache. Do not import untrusted game archives
   or replace the private native-code cache with untrusted files.

The left touch stick moves/steers; dragging the right area looks around.
Buttons change when entering vehicles. AIM toggles, MENU reveals navigation
keys, and dragging WEAPON/SWITCH/RADIO selects within wheels. A connected
controller hides the touch HUD. Android Back returns to the launcher; the
game runs in a separate process so subsequent Play starts a clean runtime.

For multiplayer, enable it and choose Host or enter another Muguet player's
address; the existing server uses UDP **47520**. This is not GTA Online.

## Known risks / test checklist

- Most phone GPUs lack BC desktop texture compression. BC1–BC7 uploads fall
  back to CPU decoding (including half-float HDR for BC6H), increasing loading
  time and texture memory considerably. Raw block reinterpretation/copies
  produced by GPU shaders are **not emulated** by this upload fallback and may
  cause rendering errors; that path needs device/game testing.
- Vulkan limits, shader features, and drivers vary. Unsupported depth formats
  fall back to Depth24PlusStencil8, but this is not a universal GPU compatibility
  layer. No OpenGL ES fallback is offered for this renderer.
- Background/resume releases and recreates Vulkan surfaces with a GPU-thread
  acknowledgement; audio is paused. Exercise Home, lock/unlock, rotation,
  controller disconnection, and repeated Play/Back cycles on a real device.
- Check Story and Sandbox startup, vehicle controls, menus, audio, saves/reload,
  texture fidelity and a two-device LAN session.
- Large games may exceed the OS memory budget even with `largeHeap`. Android
  can terminate the process; no equivalent to Apple's increased-memory
  entitlement is available. Check thermal throttling during longer sessions.
- Original limitations remain (including missing minimap and single-player
  story missions).

Logs:

```sh
adb logcat --pid=<game-process-pid>
# Obtain the PID:
adb shell pidof dev.muguet:game
```

Rust unit tests (on a supported Unix development host): `cargo test --lib`.
Compile-only Android check after configuring the NDK:
`cargo ndk -t arm64-v8a --platform 29 check --lib`.

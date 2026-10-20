# Muguet Android — experimental

This adds an **ARM64 native Android application**, not an iOS emulator or a
WebView. It uses Wasmtime for `game.wasm`, wgpu/Vulkan for graphics, CPAL/AAudio
for audio, Android views for touch controls, and the existing UDP multiplayer.
No proprietary game files are included in the APK.
The Android app is displayed as **GTA 5** (powered by Muguet). Its package ID
remains `dev.muguet`, retaining its storage layout. Installing an update requires
the same signing key; fresh CI debug keys may require uninstalling the older app
(which deletes its private game files and saves). This is not an official Rockstar app.

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
2. Open **GTA 5** and choose **Download / Resume Game**, or select **Import game ZIP**
   with your own compatible web-port archive: `data/manifest.json`,
   `b/<build>/game.wasm`, and `b/<build>/shaders/index.json`. A regular Steam/PC
   installation alone is not compatible with this runtime. See the download
   details below before starting.
3. For ZIP import, keep the launcher open. Allow space for a temporary copy
   of the ZIP plus the extracted files (and the old installation if replacing
   it). Imported data, saves and config live in app-private storage; uninstalling
   the app deletes them. Import cannot run as a background service.
4. Start at **30 FPS, 0.5 render scale, 5 GB maximum heap**. The heap cap is
   not total app memory: textures, compilation and native workers need more.
5. On Android, MSAA is capped at **2x** (desktop keeps the adapter maximum of
   up to 16x). MSAA is the largest single fragment-cost lever on phone GPUs,
   so 4x costs far more than the image quality returns. Set `MUGUET_MSAA_MAX`
   in `config.json`'s `extra_args` environment or rebuild with it set to
   override. The surface presents with `Fifo` (vsync-matched) instead of
   `Mailbox`, which avoids rendering frames the compositor never displays when
   the game outruns the display.
6. Set touch camera speed with **Look sensitivity** (0.25x–3.75x, default 1x,
   matching the previously fixed gain). It scales camera-drag look and
   select-wheel drag on the touch HUD. Movement stick strength and the game's
   own mouse setting (`mouse_sensitivity` in `config.json`) are separate and
   unchanged; controller look speed is unchanged.
5. First Play patches and compiles the Wasm on a worker thread and caches the
   generated native code. This can take several minutes and consume substantial
   RAM. Later launches reuse the cache. Do not import untrusted game archives
   or replace the private native-code cache with untrusted files.

### Automatic download/install

- Manifest: <https://gcode.bytarch.com/download-manifest.json>. Its current
  response is version 1, repository `alihammoud21/playgta5`, tag `full-game-v1`:
  **14 independent ZIP archives, 21,192,696,051 bytes** (about 21.2 GB).
  Archive URLs in that manifest point to the GitHub release, not gcode.bytarch.com.
- Download only if you are entitled to use the assets. This is an unofficial
  third-party source; asset ownership/redistribution permission is not granted
  by this app. SHA-256 verification detects changed downloads, not legal rights
  or trustworthy provenance. The APK and repository contain no game data.
- Confirm the download in the launcher. Use Wi-Fi, keep the device charged,
  and allow roughly **25 GB of free internal storage**, in addition to any
  existing game installation. Storage is checked before downloading and before
  extraction; the current manifest's initial estimate is about 24.95 GB.
- A foreground service shows download/install progress and a **Pause** action.
  Allow notifications on Android 13+ to see progress in the notification shade;
  downloading still works if that permission is declined. **Download / Resume**
  continues partial HTTP downloads and skips already installed/verified parts.
  Closing the launcher does not deliberately cancel the service. Android may
  still stop it due to OS policy, network loss, or force-stop; resume manually.
  Android 15+ allows at most six background hours per day for data-sync services.
- Each part is size-checked and SHA-256-checked before extracting, then its ZIP
  entry sizes/CRC checksums are verified. Only game data, Wasm, and shaders are
  installed: no Windows runtime, launch scripts, or browser workers are run.
  The archive is deleted after its extraction receipt is saved. There is no
  combined ZIP or duplicate full-game extraction.
- Staged files are SHA-256-checked again before replacing the active game.
  Existing game files stay intact until every part finishes. Saves/config are
  outside the game directory and are preserved. Installation renames use a
  recovery journal; tap Resume if installation was interrupted at the last step.
- **Discard partial download** removes only installer staging/downloads, not
  the installed game or saves. ZIP import is disabled until the partial download
  is discarded, avoiding conflicting installations. A checksum failure removes
  the bad archive; resume to retry. If completed staged files are damaged or the
  manifest changes, discard and start again.

The multi-gigabyte download, pause/background behavior, installation recovery,
and actual gameplay still require real-device testing. JVM tests exercise the
installer with small synthetic archives and mocked HTTP responses only.

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
  time and texture memory considerably. This is usually the dominant hitching
  source during world streaming on mid-range Mali/Adreno GPUs, and it is
  unchanged by render scale. Raw block reinterpretation/copies
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

The exported launcher log also carries `[hud]` lines describing the touch HUD
window (attached, dropped, or why attaching failed), which is the first place
to look when the touch buttons are missing.

Rust unit tests (on a supported Unix development host): `cargo test --lib`.
Installer JVM tests: `gradle -p android testDebugUnitTest` (requires the build
prerequisites above). GitHub Actions runs these before uploading the APK.
Compile-only Android check after configuring the NDK:
`cargo ndk -t arm64-v8a --platform 29 check --lib`.

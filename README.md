# Muguet
A native iOS/macOS port of GTA V, with multiplayer, plus an experimental Android target

## Android (experimental)

ARM64 Android support, ZIP import, touch/controller controls, Vulkan graphics,
and Android audio are implemented. **Device/in-game validation is still required.**
See [android/README.md](android/README.md) for requirements and limitations.

Push to `main` to build with **GitHub Actions → Android APK**; download the
`Muguet-Android-arm64-debug` artifact from the completed run.

Build with `.\make-android.ps1` on Windows or `sh make-android.sh` on Linux/macOS.
Output: `android/app/build/outputs/apk/debug/app-debug.apk`. The app is labeled
**GTA 5**, powered by Muguet (unofficial). **Download / Resume Game** installs
verified release parts using <https://gcode.bytarch.com/download-manifest.json>,
or you can import your own compatible web-port ZIP. No game files ship in the
APK; the automatic download requires roughly 25 GB free internal storage.

### Native engine core (WIP)

A separate port of the upstream [gta5-ios-port](https://github.com/kevinerikjs/gta5-ios-port)
engine core runs `game.wasm` as native ARM64 C (wasm2c) instead of interpreting
it with Wasmtime, using this project's Android shell. It requires your own game
files to generate the engine sources and is **not yet wired into the default
build**. See [app/README.md](app/README.md). The Vulkan renderer is the remaining
work; the engine currently builds but does not draw.

## How does this work ?
this whole port relies on the WebAssembly compilation of the game. in a browser, that `game.wasm` runs inside the JS engine, behind WebGPU, Web Audio, web workers...
Muguet throws that layer away and runs the same `game.wasm` as a regular native app, with [wasmtime](https://wasmtime.dev) for the code and [wgpu](https://wgpu.rs) on Metal for the graphics

On macOS, the game code is compiled to native arm64 the first time you launch it, then cached. On iOS, we can't properly do that (i dont rlly want you to deal with the hassle of enabling JIT), so the game code is compiled when you build the app and baked into it

On top of that, Muguet adds a small UDP multiplayer (for other people who have the same port to play with you)

## Game files
Because I care about me not getting sued, you will need your own copy as a `.zip` : the folder holding `data/` and `b/<build>/` (with `game.wasm` and `shaders/`)

It's basically the same source as the web port.

Muguet itself will then asks for that zip

## Requirements
- Apple Silicon Mac, macOS 14 or later
- Rust toolchain
- Xcode 26 or later

For iOS, also :
- `rustup` with `rustup toolchain install 1.98.1` and `rustup target add aarch64-apple-ios --toolchain 1.98.1`
- `xcodegen` (`brew install xcodegen`)
- an Apple developer account
- a device on iOS 15 or later, ideally 8 GB of RAM

## Building
macOS :
```
./make-app.sh
```
gives `build/Muguet.app`

iOS :
```
TEAM=<your team id> ./make-ios.sh game.zip --install
```
`--ipa` makes `build/Muguet.ipa` instead. With a free account, add `FREE=1` (+ a `BUNDLE_ID` of your own)

Please install this app with AltStore. This probably won't work well with Sideloadly (or even SideStore, as per [this issue])(https://github.com/SideStore/SideStore/issues/1616)

The app also works with a paid developer account.

## Multiplayer
One player hosts from the launcher (or runs `muguet-server` on any machine), the others join with that address. UDP port 47520

## Limitations
- iOS needs the increased memory limit entitlement
- Minimap doesn't show
- Story missions are single player only


Contributions are welcome !

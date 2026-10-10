param([switch]$Install)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
    foreach ($tool in @('cargo', 'rustup', 'gradle', 'java')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing $tool. See android/README.md for setup." }
    }
    if (-not $env:ANDROID_HOME -and -not (Test-Path 'android/local.properties')) { throw 'Set ANDROID_HOME to your Android SDK, or configure android/local.properties.' }
    & rustup target add aarch64-linux-android
    if ($LASTEXITCODE -ne 0) { throw 'Installing the Rust Android target failed.' }
    & cargo ndk --version
    if ($LASTEXITCODE -ne 0) { throw 'Install cargo-ndk first: cargo install cargo-ndk --locked' }
    & gradle -p android assembleDebug
    if ($LASTEXITCODE -ne 0) { throw 'Android build failed.' }
    $apk = Join-Path $PSScriptRoot 'android/app/build/outputs/apk/debug/app-debug.apk'
    Write-Host "Built: $apk"
    if ($Install) {
        & adb install -r $apk
        if ($LASTEXITCODE -ne 0) { throw 'APK installation failed.' }
    }
} finally { Pop-Location }

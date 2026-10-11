// muguet - config.rs
// Constantin Clerc (c22dev) - 0.1

use std::path::{Path, PathBuf};

#[derive(Clone)]
#[cfg_attr(target_os = "ios", allow(dead_code))]
pub struct Startup {
    pub wasm_path: PathBuf,
    pub cache_dir: PathBuf,

    pub engine_args: Vec<String>,
}

const DEFAULT_CONFIG: &str = r#"{
  "scale": 0.5,
  "mouse_sensitivity": 2.0,
  "fullscreen": true,
  "fps_limit": 60,
  "start_mode": 2,
  "mute": false,
  "multiplayer": false,
  "host": false,
  "server": "",
  "player_name": "",
  "extra_args": []
}
"#;

fn support_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    return PathBuf::from(std::env::var_os("MUGUET_SUPPORT").expect("Android data directory not initialized"));
    #[cfg(not(target_os = "android"))]
    {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    if cfg!(target_os = "ios") {
        return PathBuf::from(home).join("Documents");
    }
    PathBuf::from(home).join("Library/Application Support/Muguet")
    }
}

pub fn game_dir() -> PathBuf {
    support_dir().join("game")
}

pub fn game_ready() -> bool {
    let dir = game_dir();
    dir.join("data/manifest.json").exists() && build_dir(&dir).is_some_and(|b| !cfg!(feature = "jit") || crate::code_embedded() || b.join("game.wasm").exists())
}

pub fn bundled_resources() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let res = exe.parent()?.parent()?.join("Resources");
    (exe.parent()?.ends_with("Contents/MacOS") && res.is_dir()).then_some(res)
}

fn build_dir(root: &Path) -> Option<PathBuf> {
    std::fs::read_dir(root.join("b")).ok()?.flatten().map(|e| e.path()).find(|p| p.join("shaders/index.json").exists())
}

fn set_default(key: &str, value: impl AsRef<std::ffi::OsStr>) {
    if std::env::var_os(key).is_none() {
        std::env::set_var(key, value);
    }
}

#[cfg(target_os = "macos")]
fn redirect_logs_if_detached() {
    extern "C" {
        fn isatty(fd: i32) -> i32;
        fn dup2(old: i32, new: i32) -> i32;
    }

    if cfg!(target_os = "ios") {
        return;
    }
    let in_bundle = std::env::current_exe().map(|p| p.to_string_lossy().contains(".app/Contents/MacOS/")).unwrap_or(false);
    if !in_bundle || unsafe { isatty(2) } != 0 {
        return;
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = PathBuf::from(home).join("Library/Logs/Muguet.log");
    if let Ok(f) = std::fs::File::create(&path) {
        use std::os::fd::IntoRawFd;
        let fd = f.into_raw_fd();
        unsafe {
            dup2(fd, 2);
            dup2(fd, 1);
        }
    }
}

pub fn setup() -> anyhow::Result<Startup> {
    #[cfg(target_os = "macos")]
    redirect_logs_if_detached();
    let support = support_dir();
    std::fs::create_dir_all(support.join("saves"))?;

    let cfg_path = support.join("config.json");
    if !cfg_path.exists() {
        std::fs::write(&cfg_path, DEFAULT_CONFIG)?;
    }
    let engine_args = settings(&cfg_path);

    set_default("MUGUET_FSROOT", support.join("saves"));
    let build = locate();
    #[cfg(not(target_os = "ios"))]
    let build = build?;
    #[cfg(target_os = "ios")]
    let build = build.unwrap_or_default();
    Ok(Startup { wasm_path: build.join("game.wasm"), cache_dir: support, engine_args })
}

fn locate() -> anyhow::Result<PathBuf> {
    let root = game_dir();
    let build = build_dir(&root).filter(|_| game_ready()).ok_or_else(|| anyhow::anyhow!("no game data: import your game .zip from the launcher"))?;
    std::env::set_var("MUGUET_DATA", root.join("data"));
    std::env::set_var("MUGUET_SHADERS", build.join("shaders"));
    eprintln!("[config] game {}", root.display());
    Ok(build)
}

const SETTING_VARS: &[&str] = &["MUGUET_SCALE", "MUGUET_MOUSE_SENS", "MUGUET_TOUCH_SENS", "MUGUET_RES", "MUGUET_FULLSCREEN", "MUGUET_MUTE", "MUGUET_HOST", "MUGUET_SERVER", "MUGUET_NAME"];

#[cfg(target_os = "ios")]
pub fn reload() -> Vec<String> {
    for v in SETTING_VARS { std::env::remove_var(v); }
    let args = settings(&support_dir().join("config.json"));
    if let Err(e) = locate() {
        eprintln!("[config] {e:#}");
    }
    eprintln!("[config] launcher settings applied: {}", args.join(" "));
    args
}

fn settings(cfg_path: &Path) -> Vec<String> {
    let _ = SETTING_VARS;
    let cfg: serde_json::Value = std::fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).map_err(|e| eprintln!("[config] {}: {e}", cfg_path.display())).ok())
        .unwrap_or_else(|| serde_json::from_str(DEFAULT_CONFIG).unwrap());
    let num = |k: &str| cfg.get(k).and_then(|v| v.as_f64());
    let flag = |k: &str| cfg.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    if let Some(v) = num("scale") { set_default("MUGUET_SCALE", v.to_string()); }
    if let Some(v) = num("mouse_sensitivity") { set_default("MUGUET_MOUSE_SENS", v.to_string()); }
    if let Some(v) = num("touch_sensitivity") { set_default("MUGUET_TOUCH_SENS", v.to_string()); }
    if let Some(r) = cfg.get("resolution").and_then(|v| v.as_str()) { set_default("MUGUET_RES", r); }
    if flag("fullscreen") { set_default("MUGUET_FULLSCREEN", "1"); }
    if flag("mute") { set_default("MUGUET_MUTE", "1"); }
    #[cfg(target_os = "android")]
    if let Some(v) = num("android_heap_gb") { set_default("MUGUET_ANDROID_HEAP_GB", (v as u64).clamp(1, 8).to_string()); }

    let text = |k: &str| cfg.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    if flag("multiplayer") {
        if flag("host") { set_default("MUGUET_HOST", "1"); }
        else if let Some(v) = text("server") { set_default("MUGUET_SERVER", v); }
    }
    if let Some(v) = text("player_name") { set_default("MUGUET_NAME", v); }

    let mut engine_args = Vec::new();

    let fps = std::env::var("MUGUET_FPS").ok().and_then(|v| v.parse::<f64>().ok()).or_else(|| num("fps_limit")).unwrap_or(60.0);
    engine_args.push(format!("-frameLimit={}", if fps <= 0.0 { 0 } else if fps <= 30.0 { 2 } else { 1 }));

    let start_mode = std::env::var("MUGUET_START").ok().and_then(|v| v.parse::<f64>().ok()).or_else(|| num("start_mode")).unwrap_or(2.0) as i64;
    let start_mode = if (1..=2).contains(&start_mode) { start_mode } else { 2 };
    if !std::env::args().any(|a| a.starts_with("-wasmstartchoice")) {
        engine_args.push(format!("-wasmstartchoice={start_mode}"));
    }
    if let Some(extra) = cfg.get("extra_args").and_then(|v| v.as_array()) {
        engine_args.extend(extra.iter().filter_map(|a| a.as_str().map(String::from)));
    }
    fix_settings(num("texture_quality").map(|q| q as i64));
    engine_args
}

fn fix_settings(quality: Option<i64>) {
    let Some(q) = quality.map(|q| q.clamp(0, 2)) else { return };
    // The game resolves graphics settings from the user profile, falling back to
    // the copy shipped with the data. Patch both so a missing profile cannot
    // leave the desktop "High" defaults in place.
    let profile = support_dir().join("saves/userdata/Documents/Rockstar Games/GTA V/settings.xml");
    let data = game_dir().join("data/x64/data/settings.xml");
    for path in [profile, data] {
        let Ok(xml) = std::fs::read_to_string(&path) else { continue };
        let new = tune(&xml, q);
        if new != xml {
            // Clamped here rather than inside tune so the settings tests stay
            // independent of the GPU probe.
            let mut clamped = new;
            set_num(&mut clamped, "MSAA", crate::gpu::max_msaa() as f64);
            if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
            let _ = std::fs::write(&path, clamped.as_bytes());
            eprintln!("[config] applied graphics quality level {q} to {}", path.display());
        }
    }
}

const QUALITY: [&str; 3] = ["Low", "Medium", "High"];

// The game stores graphics options as named tiers (<ShadowQuality>High</
// ShadowQuality>) and scalars (<Shadow_Distance value="1.000000" />). The
// previous code only matched `<Tag value="N" />` with an integer, so no tier
// and no decimal value was ever rewritten.
fn tune(xml: &str, q: i64) -> String {
    let mut out = xml.to_string();
    let tier = QUALITY[q as usize];
    let low = q == 0;
    for target in ["Tessellation", "ShadowQuality", "ReflectionQuality", "ParticleQuality", "WaterQuality", "LightingQuality", "GrassQuality", "TextureQuality"] {
        set_tier(&mut out, target, tier);
    }
    set_tier(&mut out, "Shadow_SoftShadows", if low { "Medium" } else { tier });
    set_tier(&mut out, "FXAA", if low { "Low" } else { tier });
    set_bool(&mut out, "SSAO", !low);
    set_bool(&mut out, "Shadow_ParticleShadows", !low);
    set_bool(&mut out, "Reflection_MipBlur", !low);
    set_bool(&mut out, "Lighting_FogVolumes", !low);
    set_bool(&mut out, "Shadow_LongShadows", false);
    set_num(&mut out, "Shadow_Distance", if low { 0.45 } else if q == 1 { 0.7 } else { 1.0 });
    // LodScale is the render-distance lever the engine actually exposes here:
    // lower values move object detail to coarser LODs sooner, which cuts the
    // object and draw count in open terrain.
    set_num(&mut out, "LodScale", if low { 0.65 } else if q == 1 { 0.85 } else { 1.0 });
    // ShaderQuality is deliberately untouched: a lower tier has been observed
    // to fail shader-variable lookup on startup.
    out
}

fn set_tier(xml: &mut String, tag: &str, value: &str) {
    let re = regex::Regex::new(&format!(r#"(<{tag}>)[A-Za-z0-9 ]+(</{tag}>)"#)).unwrap();
    *xml = re.replace_all(xml, format!("${{1}}{value}${{2}}")).into_owned();
}

fn set_bool(xml: &mut String, tag: &str, value: bool) {
    let re = regex::Regex::new(&format!(r#"(<{tag} value=")(?:true|false)(" ?/>)"#)).unwrap();
    *xml = re.replace_all(xml, format!("${{1}}{value}${{2}}")).into_owned();
}

fn set_num(xml: &mut String, tag: &str, value: f64) {
    let re = regex::Regex::new(&format!(r#"(<{tag} value=")-?\d+(?:\.\d+)?(" ?/>)"#)).unwrap();
    *xml = re.replace_all(xml, format!("${{1}}{value}${{2}}")).into_owned();
}

#[cfg(test)]
mod tests {
    use super::*;

    const SETTINGS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<configSource>SMC_AUTO</configSource>
<Settings>
  <graphics>
    <Tessellation>High</Tessellation>
    <LodScale value="1.000000" />
    <ShadowQuality>High</ShadowQuality>
    <ReflectionQuality>High</ReflectionQuality>
    <SSAO value="true" />
    <MSAA value="4" />
    <TextureQuality>High</TextureQuality>
    <ParticleQuality>High</ParticleQuality>
    <WaterQuality>High</WaterQuality>
    <LightingQuality>High</LightingQuality>
    <GrassQuality>High</GrassQuality>
    <ShaderQuality>High</ShaderQuality>
    <Shadow_SoftShadows>Medium</Shadow_SoftShadows>
    <Shadow_ParticleShadows value="true" />
    <Shadow_Distance value="1.000000" />
    <Shadow_LongShadows value="false" />
    <Reflection_MipBlur value="true" />
    <FXAA>High</FXAA>
    <Lighting_FogVolumes value="true" />
    <MulticoreRendering value="false" />
  </graphics>
  <video><Windowed value="false" /></video>
</Settings>
"#;

    // Reads either <Tag value="N" /> or <Tag>Text</Tag>.
    fn field(xml: &str, tag: &str) -> String {
        let re = regex::Regex::new(&format!(r#"<{tag}(?: value="([^"]*)")? ?(?:/>|>([^<]*)</{tag}>)"#)).unwrap();
        let caps = re.captures(xml).unwrap_or_else(|| panic!("missing {tag}"));
        caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str().to_string()).unwrap()
    }

    #[test]
    fn named_quality_tiers_are_rewritten() {
        let low = tune(SETTINGS, 0);
        for tag in ["ShadowQuality", "GrassQuality", "TextureQuality", "ParticleQuality", "WaterQuality", "LightingQuality", "ReflectionQuality", "Tessellation"] {
            assert_eq!(field(&low, tag), "Low", "{tag}");
        }
        let medium = tune(SETTINGS, 1);
        assert_eq!(field(&medium, "ShadowQuality"), "Medium");
        assert_eq!(field(&medium, "GrassQuality"), "Medium");
        assert_eq!(field(&tune(SETTINGS, 2), "WaterQuality"), "High");
    }

    #[test]
    fn shader_quality_is_left_at_high() {
        // Lower ShaderQuality tiers have been observed to fail shader-variable lookup.
        for q in 0..3 { assert_eq!(field(&tune(SETTINGS, q), "ShaderQuality"), "High"); }
    }

    #[test]
    fn decimal_and_boolean_knobs_are_rewritten() {
        let low = tune(SETTINGS, 0);
        assert_eq!(field(&low, "LodScale"), "0.65");
        assert_eq!(field(&low, "Shadow_Distance"), "0.45");
        assert_eq!(field(&low, "SSAO"), "false");
        assert_eq!(field(&low, "Shadow_ParticleShadows"), "false");
        assert_eq!(field(&low, "Reflection_MipBlur"), "false");
        assert_eq!(field(&low, "Lighting_FogVolumes"), "false");
        assert_eq!(field(&low, "Shadow_LongShadows"), "false");
        let medium = tune(SETTINGS, 1);
        assert_eq!(field(&medium, "LodScale"), "0.85");
        assert_eq!(field(&medium, "SSAO"), "true");
        assert_eq!(field(&medium, "Shadow_Distance"), "0.7");
        let high = tune(SETTINGS, 2);
        assert_eq!(field(&high, "LodScale"), "1");
        assert_eq!(field(&high, "Shadow_Distance"), "1");
        assert_eq!(field(&high, "SSAO"), "true");
        assert_eq!(field(&high, "Shadow_ParticleShadows"), "true");
    }

    #[test]
    fn tuning_is_idempotent_and_leaves_other_tags_alone() {
        let once = tune(SETTINGS, 0);
        assert_eq!(tune(&once, 0), once);
        assert_eq!(field(&once, "Windowed"), "false");
        assert_eq!(field(&once, "MulticoreRendering"), "false");
        // MSAA clamping happens outside tune; the tier and value must otherwise survive.
        assert_eq!(field(&once, "MSAA"), "4");
        assert_eq!(field(&once, "Shadow_SoftShadows"), "Medium");
        assert_eq!(field(&once, "FXAA"), "Low");
    }
}

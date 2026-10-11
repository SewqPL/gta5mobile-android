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
  "extra_args": [],
  "phone_lod": true,
  "phone_graphics": false
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

    // The game rewrites LodScale, PedLodBias and VehicleLodBias back to its own
    // defaults every time it saves, so any edit made by hand is lost on the next
    // save. Re-applying them at launch is the only thing that makes the LOD
    // setting stick. Measured on a Dimensity 8000: LOD at 0 drew 579 calls per
    // frame at 24.3 fps, while the shipped defaults (1.0 / 0.2 / 0.15) drew 2082
    // at 17.1 fps. The zeros are not a corrupted value, they force aggressive
    // level of detail, which is what a phone wants when draw calls are the
    // bottleneck. Do not "fix" these back to the shipped defaults.
    let phone_lod = cfg.get("phone_lod").and_then(|v| v.as_bool()).unwrap_or(true);

    // Shadow cascades and scene reflections are separate passes that each
    // redraw the scene, and together they were ~2650 of 3228 draw calls per
    // frame. That is a large win, but it visibly changes how the world looks,
    // so it is opt-in rather than on by default. Set "phone_graphics": true to
    // trade the lighting for the frame rate.
    let phone_graphics = cfg.get("phone_graphics").and_then(|v| v.as_bool()).unwrap_or(false);

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
    fix_settings(num("texture_quality").map(|q| q as i64), phone_graphics, phone_lod);
    engine_args
}

fn fix_settings(texture_quality: Option<i64>, phone_graphics: bool, phone_lod: bool) {
    let path = support_dir().join("saves/userdata/Documents/Rockstar Games/GTA V/settings.xml");
    let Ok(xml) = std::fs::read_to_string(&path) else { return };
    let new = apply_settings(&xml, texture_quality, phone_graphics, phone_lod, crate::gpu::max_msaa() as i64);
    if new != xml {
        let _ = std::fs::write(&path, new.as_bytes());
    }
}

/// Returns the settings XML with the tags this port pins adjusted, logging each
/// change. Split out from fix_settings so the substitutions are testable without
/// a device or a writable support directory.
///
/// Values are matched and rewritten preserving the file's own formatting: an
/// integer tag stays an integer, and a 6-decimal tag keeps 6 decimals. The game
/// writes some tags as "0" and others as "0.000000", and rendering them back in
/// a different shape would churn the save on every launch.
fn apply_settings(xml: &str, texture_quality: Option<i64>, phone_graphics: bool, phone_lod: bool, max_msaa: i64) -> String {
    let mut new = xml.to_string();
    let mut set = |tag: &str, fix: &dyn Fn(f64) -> f64| {
        let re = regex::Regex::new(&format!(r#"<{tag} value="(-?\d+(?:\.\d+)?)"\s*/>"#)).unwrap();
        let Some(captured) = re.captures(&new) else { return };
        let raw = captured[1].to_string();
        let Ok(v) = raw.parse::<f64>() else { return };
        let to = fix(v);
        if to != v {
            let rendered = match raw.split_once('.') {
                Some((_, frac)) => format!("{:.*}", frac.len(), to),
                None => format!("{}", to as i64),
            };
            new = re.replace(&new, format!(r#"<{tag} value="{rendered}" />"#).as_str()).into_owned();
            eprintln!("[config] {tag} {raw} -> {rendered}");
        }
    };
    if let Some(q) = texture_quality {
        set("TextureQuality", &|_| q.clamp(0, 2) as f64);
    }
    if phone_graphics {
        // Opt-in: visibly changes how the world looks.
        set("ShadowQuality", &|_| 0.0);
        set("ReflectionQuality", &|_| 0.0);
    }
    if phone_lod {
        // On by default, and must be re-applied every launch because the game
        // resets these on save. See the note where phone_lod is read.
        set("LodScale", &|_| 0.0);
        set("PedLodBias", &|_| 0.0);
        set("VehicleLodBias", &|_| 0.0);
    }
    set("ShaderQuality", &|v| v.max(1.0));
    set("MSAA", &|v| v.min(max_msaa as f64));
    new
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<Config>
  <TextureQuality value="2" />
  <ShadowQuality value="1" />
  <ReflectionQuality value="1" />
  <ShaderQuality value="0" />
  <MSAA value="4" />
  <LodScale value="1.000000" />
  <PedLodBias value="0.200000" />
  <VehicleLodBias value="0.150000" />
</Config>"#;

    fn tag(xml: &str, name: &str) -> String {
        let re = regex::Regex::new(&format!(r#"<{name} value="[^"]*"\s*/>"#)).unwrap();
        re.captures(xml).unwrap()[0].to_string()
    }

    #[test]
    fn phone_lod_alone_leaves_the_lighting_alone() {
        // The default: LOD pinned, shadows and reflections untouched.
        let out = apply_settings(SAMPLE, None, false, true, 1);
        assert_eq!(tag(&out, "LodScale"), r#"<LodScale value="0.000000" />"#);
        assert_eq!(tag(&out, "PedLodBias"), r#"<PedLodBias value="0.000000" />"#);
        assert_eq!(tag(&out, "VehicleLodBias"), r#"<VehicleLodBias value="0.000000" />"#);
        assert_eq!(tag(&out, "ShadowQuality"), r#"<ShadowQuality value="1" />"#);
        assert_eq!(tag(&out, "ReflectionQuality"), r#"<ReflectionQuality value="1" />"#);
    }

    #[test]
    fn phone_graphics_drops_the_scene_redraw_passes() {
        let out = apply_settings(SAMPLE, None, true, true, 1);
        assert_eq!(tag(&out, "ShadowQuality"), r#"<ShadowQuality value="0" />"#);
        assert_eq!(tag(&out, "ReflectionQuality"), r#"<ReflectionQuality value="0" />"#);
        // Texture quality is untouched unless explicitly requested.
        assert_eq!(tag(&out, "TextureQuality"), r#"<TextureQuality value="2" />"#);
    }

    #[test]
    fn phone_graphics_pins_lod_to_zero_keeping_six_decimals() {
        let out = apply_settings(SAMPLE, None, true, true, 1);
        assert_eq!(tag(&out, "LodScale"), r#"<LodScale value="0.000000" />"#);
        assert_eq!(tag(&out, "PedLodBias"), r#"<PedLodBias value="0.000000" />"#);
        assert_eq!(tag(&out, "VehicleLodBias"), r#"<VehicleLodBias value="0.000000" />"#);
    }

    #[test]
    fn the_two_flags_are_independent() {
        // Shadows off, LOD left at the game's defaults.
        let no_lod = apply_settings(SAMPLE, None, true, false, 1);
        assert_eq!(tag(&no_lod, "ShadowQuality"), r#"<ShadowQuality value="0" />"#);
        assert_eq!(tag(&no_lod, "LodScale"), r#"<LodScale value="1.000000" />"#);

        // LOD on, shadows left alone.
        let no_graphics = apply_settings(SAMPLE, None, false, true, 1);
        assert_eq!(tag(&no_graphics, "ShadowQuality"), r#"<ShadowQuality value="1" />"#);
        assert_eq!(tag(&no_graphics, "LodScale"), r#"<LodScale value="0.000000" />"#);
    }

    #[test]
    fn integer_tags_stay_integers() {
        // The decimal support must not reshape the tags the game writes as ints.
        let out = apply_settings(SAMPLE, None, true, true, 1);
        assert!(!out.contains(r#"<ShadowQuality value="0.0" />"#));
        assert!(!out.contains(r#"<MSAA value="1.0" />"#));
        assert!(out.contains(r#"<ShadowQuality value="0" />"#));
    }

    #[test]
    fn lod_already_at_zero_is_left_untouched() {
        // Steady state: once the port has pinned these, every later launch must
        // find them already correct, otherwise the save is rewritten forever.
        // The fixture must already satisfy every rule apply_settings enforces,
        // including the MSAA cap, or this compares a changed file to an
        // unchanged one and fails for the wrong reason.
        let zeros = SAMPLE
            .replace(r#"<LodScale value="1.000000" />"#, r#"<LodScale value="0.000000" />"#)
            .replace(r#"<PedLodBias value="0.200000" />"#, r#"<PedLodBias value="0.000000" />"#)
            .replace(r#"<VehicleLodBias value="0.150000" />"#, r#"<VehicleLodBias value="0.000000" />"#)
            .replace(r#"<ShaderQuality value="0" />"#, r#"<ShaderQuality value="1" />"#)
            .replace(r#"<MSAA value="4" />"#, r#"<MSAA value="1" />"#);
        assert_eq!(apply_settings(&zeros, None, false, true, 1), zeros);
    }

    #[test]
    fn shader_quality_is_floored_and_msaa_capped_at_the_device_max() {
        let out = apply_settings(SAMPLE, None, false, true, 2);
        assert_eq!(tag(&out, "ShaderQuality"), r#"<ShaderQuality value="1" />"#);
        assert_eq!(tag(&out, "MSAA"), r#"<MSAA value="2" />"#);
    }

    #[test]
    fn requested_texture_quality_is_applied_and_clamped() {
        assert_eq!(
            tag(&apply_settings(SAMPLE, Some(0), false, true, 1), "TextureQuality"),
            r#"<TextureQuality value="0" />"#
        );
        assert_eq!(
            tag(&apply_settings(SAMPLE, Some(9), false, true, 1), "TextureQuality"),
            r#"<TextureQuality value="2" />"#
        );
    }

    #[test]
    fn missing_tags_are_left_missing() {
        let sparse = "<Config>\n  <ShadowQuality value=\"1\" />\n</Config>";
        let out = apply_settings(sparse, Some(0), true, true, 1);
        assert!(!out.contains("TextureQuality"));
        assert!(!out.contains("ReflectionQuality"));
        assert!(out.contains(r#"<ShadowQuality value="0" />"#));
    }
}

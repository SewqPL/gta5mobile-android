use anyhow::{Context, Result};
use jni::objects::{JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jfloat, jint, jlong, JNI_FALSE, JNI_TRUE};
use jni::{JNIEnv, JavaVM};
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use winit::platform::android::activity::AndroidApp;

static ACTIVITY: OnceLock<AndroidApp> = OnceLock::new();
static HELD: Mutex<Option<HashSet<String>>> = Mutex::new(None);

pub fn activity() -> AndroidApp { ACTIVITY.get().expect("android_main not initialized").clone() }

pub fn status(text: &str) {
    let Some(app) = ACTIVITY.get() else { eprintln!("[android] {text}"); return };
    let result = (|| -> Result<()> {
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }?;
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        let text = env.new_string(text)?;
        env.call_method(&activity, "setStatus", "(Ljava/lang/String;)V", &[JValue::Object(text.as_ref())])?;
        Ok(())
    })();
    if let Err(e) = result { eprintln!("[android] status: {e:#}"); }
}

#[no_mangle]
fn android_main(app: AndroidApp) {
    ACTIVITY.set(app.clone()).ok();
    let result = (|| -> Result<()> {
        let support = app.internal_data_path().context("no internal Android storage")?;
        std::env::set_var("MUGUET_SUPPORT", support);
        crate::raise_open_file_limit();
        let startup = crate::config::setup()?;
        status("Compiling game on first launch. This may take several minutes…");
        let args = crate::game_args(&startup);
        crate::app::run_android(startup, args, app)
    })();
    if let Err(e) = result { status(&format!("Startup failed: {e:#}")); eprintln!("[android] {e:#}"); }
}

fn java_error(env: &mut JNIEnv, error: impl std::fmt::Display) {
    let _ = env.throw_new("java/lang/IllegalStateException", error.to_string());
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_LauncherActivity_nativePrepare(mut env: JNIEnv, _class: JClass, directory: JString) -> jboolean {
    let result = (|| -> Result<bool> {
        let path: String = env.get_string(&directory)?.into();
        std::env::set_var("MUGUET_SUPPORT", &path);
        std::fs::create_dir_all(&path)?;
        Ok(crate::config::game_ready())
    })();
    match result { Ok(true) => JNI_TRUE, Ok(false) => JNI_FALSE, Err(e) => { java_error(&mut env, e); JNI_FALSE } }
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_LauncherActivity_nativeImport(mut env: JNIEnv, activity: JObject, archive: JString) {
    let result = (|| -> Result<()> {
        let path: String = env.get_string(&archive)?.into();
        crate::import::import(std::path::Path::new(&path), &crate::config::game_dir(), true, |done, total| {
            let _ = env.call_method(&activity, "importProgress", "(JJ)V", &[JValue::Long(done as i64), JValue::Long(total as i64)]);
        })
    })();
    if let Err(e) = result { java_error(&mut env, format!("{e:#}")); }
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_GameActivity_nativeInput(mut env: JNIEnv, _class: JClass, keys: JObjectArray, buttons: jint, horn: jboolean, controller: jboolean) {
    let result = (|| -> Result<()> {
        let mut next = HashSet::new();
        for i in 0..env.get_array_length(&keys)? {
            let value = JString::from(env.get_object_array_element(&keys, i)?);
            next.insert(env.get_string(&value)?.into());
            env.delete_local_ref(value)?;
        }
        let mut guard = HELD.lock().unwrap();
        let previous = guard.get_or_insert_with(HashSet::new);
        for key in previous.difference(&next) { crate::input::key(key, false, false, None, false); }
        for key in next.difference(previous) {
            crate::mp::key_pressed(key);
            crate::input::key(key, true, false, None, false);
        }
        *previous = next;
        for (bit, dom) in [(1, 0), (2, 2)] { crate::input::button(dom, buttons & bit != 0); }
        crate::pad::HORN_TOUCH.store(horn != 0, Ordering::Relaxed);
        crate::pad::ACTIVE.store(controller != 0, Ordering::Relaxed);
        crate::input::set_locked(true);
        Ok(())
    })();
    if let Err(e) = result { java_error(&mut env, e); }
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_GameActivity_nativeLook(_env: JNIEnv, _class: JClass, dx: jfloat, dy: jfloat) {
    if dx.is_finite() && dy.is_finite() { crate::input::mouse_delta_f(dx as f64, dy as f64); }
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_GameActivity_nativeInVehicle(_env: JNIEnv, _class: JClass) -> jboolean {
    if crate::pad::IN_VEHICLE.load(Ordering::Relaxed) { JNI_TRUE } else { JNI_FALSE }
}

#[no_mangle]
pub extern "system" fn Java_dev_muguet_GameActivity_nativeHasFrame(_env: JNIEnv, _class: JClass) -> jboolean {
    if crate::gpu::PRESENTS.load(Ordering::Relaxed) > 0 { JNI_TRUE } else { JNI_FALSE }
}

// Total presented frames; the HUD derives the FPS display from its rate.
#[no_mangle]
pub extern "system" fn Java_dev_muguet_GameActivity_nativePresents(_env: JNIEnv, _class: JClass) -> jlong {
    crate::gpu::PRESENTS.load(Ordering::Relaxed) as jlong
}

pub fn release_input() {
    if let Some(held) = HELD.lock().unwrap().take() {
        for key in held { crate::input::key(&key, false, false, None, false); }
    }
    crate::input::button(0, false);
    crate::input::button(2, false);
    crate::pad::HORN_TOUCH.store(false, Ordering::Relaxed);
    crate::pad::ACTIVE.store(false, Ordering::Relaxed);
}

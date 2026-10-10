// Android controllers and touch controls are translated by GameActivity.java.
use std::sync::atomic::{AtomicBool, Ordering};

pub static ACTIVE: AtomicBool = AtomicBool::new(false);
pub static IN_VEHICLE: AtomicBool = AtomicBool::new(false);
pub static HORN_PAD: AtomicBool = AtomicBool::new(false);
pub static HORN_TOUCH: AtomicBool = AtomicBool::new(false);

pub fn horn_held() -> bool {
    HORN_PAD.load(Ordering::Relaxed) || HORN_TOUCH.load(Ordering::Relaxed)
}

pub fn poll() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

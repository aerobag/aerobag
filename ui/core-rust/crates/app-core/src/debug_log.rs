use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;

pub type CoreDebugLogger = fn(&str, &Value);
pub type CoreClockMs = fn() -> f64;

static CORE_DEBUG_LOGGER: OnceLock<Mutex<Option<CoreDebugLogger>>> = OnceLock::new();
static CORE_CLOCK_MS: OnceLock<Mutex<Option<CoreClockMs>>> = OnceLock::new();

pub fn set_core_debug_logger(logger: Option<CoreDebugLogger>) {
    *CORE_DEBUG_LOGGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("core debug logger poisoned") = logger;
}

pub fn set_core_clock_ms(clock: Option<CoreClockMs>) {
    *CORE_CLOCK_MS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("core clock poisoned") = clock;
}

pub fn core_clock_ms() -> Option<f64> {
    CORE_CLOCK_MS
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("core clock poisoned")
        .map(|clock| clock())
}

pub fn core_debug_log<T: Serialize>(tag: &str, data: &T) {
    let Ok(value) = serde_json::to_value(data) else {
        return;
    };
    core_debug_log_value(tag, &value);
}

pub fn core_debug_log_value(tag: &str, data: &Value) {
    if let Some(logger) = *CORE_DEBUG_LOGGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .expect("core debug logger poisoned")
    {
        logger(tag, data);
    }
}

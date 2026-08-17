// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use serde_json::Value;

pub type CoreDebugLogger = fn(&str, &Value);
pub type CoreClockMs = fn() -> f64;

static CORE_DEBUG_LOGGER: OnceLock<Mutex<Option<CoreDebugLogger>>> = OnceLock::new();
static CORE_CLOCK_MS: OnceLock<Mutex<Option<CoreClockMs>>> = OnceLock::new();

static CORE_VERBOSE_PERF_LOGS: AtomicBool = AtomicBool::new(false);

pub fn set_core_verbose_perf_logs(enabled: bool) {
    CORE_VERBOSE_PERF_LOGS.store(enabled, Ordering::Relaxed);
}

pub fn set_core_debug_logger(logger: Option<CoreDebugLogger>) {
    *CORE_DEBUG_LOGGER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = logger;
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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
    {
        logger(tag, data);
    }
}

#[inline(always)]
pub fn core_perf_debug_log<T, F>(tag: &str, data: F)
where
    T: Serialize,
    F: FnOnce() -> T,
{
    if !CORE_VERBOSE_PERF_LOGS.load(Ordering::Relaxed) {
        return;
    }
    core_debug_log(tag, &data());
}

#[derive(Debug, Clone, Copy)]
pub struct CoreDebugTimer {
    started_ms: Option<f64>,
}

impl CoreDebugTimer {
    pub fn start() -> Self {
        Self {
            started_ms: core_clock_ms(),
        }
    }

    pub fn elapsed_ms(self) -> Option<f64> {
        core_debug_elapsed_ms_since(self.started_ms)
    }
}

pub fn core_debug_elapsed_ms_since(started_ms: Option<f64>) -> Option<f64> {
    let started_ms = started_ms?;
    Some((core_clock_ms()? - started_ms).max(0.0).round())
}

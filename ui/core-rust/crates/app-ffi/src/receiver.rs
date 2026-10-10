// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::{get_java_string, return_string};
use app_core::receiver::{capture::CaptureClock, runtime};
use jni::{
    objects::{JByteArray, JClass, JString},
    sys::jstring,
    JNIEnv,
};

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_initializeReceiver(
    mut env: JNIEnv,
    _class: JClass,
    root: JString,
) {
    let result = get_java_string(&mut env, root)
        .and_then(|root| runtime::initialize(std::path::Path::new(&root)));
    if let Err(error) = result {
        let _ = env.throw_new("java/lang/IllegalStateException", error);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_receiverHostEventJson(
    mut env: JNIEnv,
    _class: JClass,
    event: JString,
    bytes: JByteArray,
    monotonic_ms: i64,
    wall_ms: i64,
) -> jstring {
    let result = (|| {
        let event = get_java_string(&mut env, event)?;
        let event = serde_json::from_str(&event).map_err(|e| e.to_string())?;
        let bytes = env.convert_byte_array(bytes).map_err(|e| e.to_string())?;
        let output = runtime::update(
            event,
            &bytes,
            CaptureClock {
                monotonic_ms: monotonic_ms as u64,
                wall_epoch_ms: wall_ms,
            },
        )?;
        serde_json::to_string(&output).map_err(|e| e.to_string())
    })();
    return_string(&mut env, result)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_newReceiverConsumer(
    _env: JNIEnv,
    _class: JClass,
) -> i64 {
    runtime::new_session_consumer() as i64
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_attachReceiverSession(
    mut env: JNIEnv,
    _class: JClass,
    consumer: i64,
) {
    if let Err(error) = runtime::attach_session(consumer as u64) {
        let _ = env.throw_new("java/lang/IllegalStateException", error);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_beginReceiverDelivery(
    _env: JNIEnv,
    _class: JClass,
    consumer: i64,
) -> i64 {
    runtime::begin_delivery(consumer as u64)
        .map(|delivery| delivery.id as i64)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_acknowledgeReceiverDelivery(
    _env: JNIEnv,
    _class: JClass,
    id: i64,
) {
    if let Ok(delivery) = runtime::delivery(id as u64) {
        runtime::acknowledge_delivery(&delivery);
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_aerobag_app_domain_NativeBindings_applyReceiverDeliveryInSessionJson(
    mut env: JNIEnv,
    _class: JClass,
    handle: i64,
    id: i64,
    monotonic_ms: i64,
    wall_ms: i64,
) -> jstring {
    let result = (|| {
        let delivery = runtime::delivery(id as u64)?;
        let outcome = app_core::session::apply_receiver_delivery_in_session(
            handle as u32,
            &delivery,
            CaptureClock {
                monotonic_ms: monotonic_ms as u64,
                wall_epoch_ms: wall_ms,
            },
        )
        .map_err(|e| e.to_string())?;
        serde_json::to_string(&outcome).map_err(|e| e.to_string())
    })();
    return_string(&mut env, result)
}

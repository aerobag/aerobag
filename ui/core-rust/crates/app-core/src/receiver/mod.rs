// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Receive-only avionics input. No OS IO and no transponder configuration.
//! Capture bytes before calling these decoders; errors must not destroy evidence.

pub mod capture;
#[cfg(not(target_arch = "wasm32"))]
pub mod capture_files;
#[cfg(not(target_arch = "wasm32"))]
pub mod capture_replay;
pub mod connection;
mod framing;
pub mod live;
mod nmea;
mod products;
mod protocol;
pub mod radar;
#[cfg(not(target_arch = "wasm32"))]
pub mod runtime;
#[cfg(test)]
pub(crate) mod test_support;
mod weather;
#[cfg(not(target_arch = "wasm32"))]
mod weather_cache;

pub use framing::{ApplicationRecord, ApplicationStream, Frame, Framer};
pub use products::{decode_product, Attitude, Product, Traffic, TrafficOwnship, TrafficTarget};
pub use protocol::{Credentials, Protocol, ProtocolOutput, ProtocolPhase, ReceivedMessage};
pub use weather::{PartialUtc, Radar, TextReport, Weather, WeatherKind};

pub const MAX_PRODUCT_BYTES: usize = 32 * 1024 * 1024;
pub const SERVICE_UUID: &str = "58e1f790-aa26-11e3-a5e2-0800200c9a66";

/// An immutable, bounded publication retained until the session accepts it.
pub struct SessionDelivery {
    pub id: u64,
    pub panel: app_ui_contracts::receiver::UiReceiverPanel,
    pub reports: Vec<crate::weather_sources::StationReport>,
    pub live: std::sync::Arc<live::LiveSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("receiver input: {0}")]
pub struct DecodeError(pub &'static str);

type Result<T> = std::result::Result<T, DecodeError>;

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(DecodeError("size overflow"))?;
        let data = self
            .bytes
            .get(self.offset..end)
            .ok_or(DecodeError("truncated product"))?;
        self.offset = end;
        Ok(data)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<Option<f64>> {
        let value = f32::from_bits(self.u32()?) as f64;
        Ok(value.is_finite().then_some(value))
    }
    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn text(&mut self, count: usize) -> Result<String> {
        Ok(String::from_utf8_lossy(self.take(count)?)
            .trim_matches(['\0', ' ', '\r', '\n', '\t'])
            .to_string())
    }
}

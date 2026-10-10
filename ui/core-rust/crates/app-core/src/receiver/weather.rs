// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use flate2::{Decompress, FlushDecompress, Status};
use serde::{Deserialize, Serialize};

use super::{DecodeError, Reader, Result, MAX_PRODUCT_BYTES};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialUtc {
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

impl PartialUtc {
    fn read(input: &mut Reader<'_>) -> Result<Self> {
        let value = Self {
            day: input.u8()?,
            hour: input.u8()?,
            minute: input.u8()?,
        };
        if !(1..=31).contains(&value.day) || value.hour > 23 || value.minute > 59 {
            return Err(DecodeError("invalid partial UTC time"));
        }
        Ok(value)
    }

    /// Recover the missing month/year without judging report freshness. A date
    /// ahead of the anchor stays ahead; do not reinterpret it as last month.
    pub fn resolve_nearest(self, anchor: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let month = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1)?;
        [
            month.checked_sub_months(Months::new(1)),
            Some(month),
            month.checked_add_months(Months::new(1)),
        ]
        .into_iter()
        .flatten()
        .filter_map(|month| NaiveDate::from_ymd_opt(month.year(), month.month(), self.day.into()))
        .filter_map(|date| date.and_hms_opt(self.hour.into(), self.minute.into(), 0))
        .map(|date| date.and_utc())
        .min_by_key(|date| anchor.timestamp_millis().abs_diff(date.timestamp_millis()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherKind {
    Metar,
    GraphicalMetar,
    Pirep,
    Taf,
    Winds,
    Notam,
    Airmet,
    Sigmet,
    Sua,
    RegionalRadar,
    ConusRadar,
}

impl WeatherKind {
    pub fn from_message_id(id: u32) -> Option<Self> {
        Some(match id {
            0x20000010 => Self::Metar,
            0x20000011 => Self::GraphicalMetar,
            0x20000020 => Self::Pirep,
            0x20000030 => Self::Taf,
            0x20000040 => Self::Winds,
            0x20000050 => Self::Notam,
            0x20000060 => Self::Airmet,
            0x20000070 => Self::Sigmet,
            0x20000080 => Self::Sua,
            0x20000090 => Self::RegionalRadar,
            0x200000a0 => Self::ConusRadar,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextReport {
    pub text: String,
    pub station: Option<String>,
    pub notam_identifier: Option<String>,
    pub record_type_raw: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Weather {
    Empty {
        kind: WeatherKind,
    },
    Text {
        kind: WeatherKind,
        issue_time: PartialUtc,
        reports: Vec<TextReport>,
        additional_segments: Vec<Vec<u8>>,
    },
    Radar(Radar),
    Uninterpreted {
        kind: WeatherKind,
        status: [u8; 4],
        segments: Vec<Vec<u8>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Radar {
    pub kind: WeatherKind,
    pub issue_time: PartialUtc,
    pub precipitation_time: PartialUtc,
    pub center_latitude: f64,
    pub center_longitude: f64,
    pub reference_latitude: f64,
    pub columns: u16,
    pub rows: u16,
    pub pixel_width_meters: u16,
    pub pixel_height_meters: u16,
    /// Row-major source codes, not colors or a fabricated reflectivity scale.
    pub grid: Vec<u8>,
}

impl Radar {
    /// Geographic outer-edge mapping. Not an affine mapping to Web Mercator.
    pub fn lat_lon(&self, column: f64, row: f64) -> (f64, f64) {
        let dx = (f64::from(self.pixel_width_meters)
            / (6_378_137.0 * self.reference_latitude.to_radians().cos()))
        .to_degrees();
        let dy = (f64::from(self.pixel_height_meters) / 6_378_137.0).to_degrees();
        (
            self.center_latitude - (row - f64::from(self.rows) / 2.0) * dy,
            self.center_longitude + (column - f64::from(self.columns) / 2.0) * dx,
        )
    }
}

fn inflate(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut decoder = Decompress::new(true);
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let before_in = decoder.total_in();
        let before_out = decoder.total_out();
        let status = decoder
            .decompress(
                &bytes[before_in as usize..],
                &mut buffer,
                FlushDecompress::None,
            )
            .map_err(|_| DecodeError("invalid zlib stream"))?;
        let count = (decoder.total_out() - before_out) as usize;
        if output.len() + count > limit {
            return Err(DecodeError("weather expansion limit"));
        }
        output.extend_from_slice(&buffer[..count]);
        if status == Status::StreamEnd {
            if decoder.total_in() != bytes.len() as u64 {
                return Err(DecodeError("trailing zlib data"));
            }
            return Ok(output);
        }
        if before_in == decoder.total_in() && count == 0 {
            return Err(DecodeError("truncated zlib data"));
        }
    }
}

pub(super) fn decode_weather(kind: WeatherKind, bytes: &[u8]) -> Result<Weather> {
    if bytes.is_empty() {
        return Ok(Weather::Empty { kind });
    }
    if bytes.len() > MAX_PRODUCT_BYTES {
        return Err(DecodeError("weather file too large"));
    }
    let mut input = Reader::new(bytes);
    let status: [u8; 4] = input.take(4)?.try_into().unwrap();
    if status[0] != 1 || status[1] > 1 || status[2] > 1 || status[3] != 0 {
        return Err(DecodeError("unsupported weather container"));
    }
    let declared = input.u32()? as usize;
    if declared > MAX_PRODUCT_BYTES {
        return Err(DecodeError("declared weather expansion limit"));
    }
    let mut sizes = [0usize; 16];
    for size in &mut sizes {
        *size = input.u32()? as usize;
    }
    let total = sizes
        .iter()
        .try_fold(72usize, |sum, n| sum.checked_add(*n))
        .ok_or(DecodeError("segment size overflow"))?;
    if total != bytes.len() {
        return Err(DecodeError("weather segment sizes"));
    }
    let mut parts = Vec::new();
    for size in sizes {
        if size != 0 {
            parts.push(input.take(size)?);
        }
    }
    if status[2] != 1 || matches!(kind, WeatherKind::GraphicalMetar | WeatherKind::Sua) {
        return Ok(Weather::Uninterpreted {
            kind,
            status,
            segments: parts.iter().map(|part| part.to_vec()).collect(),
        });
    }
    if matches!(kind, WeatherKind::RegionalRadar | WeatherKind::ConusRadar) {
        return decode_radar(kind, &parts).map(Weather::Radar);
    }
    let (entry_size, pointer_offset, max_text, count_offset, count_size) = match kind {
        WeatherKind::Metar => (77, 69, 4096, 3, 2),
        WeatherKind::Taf => (12, 4, 6095, 3, 4),
        WeatherKind::Pirep => (92, 84, 4096, 3, 2),
        WeatherKind::Winds => (20, 12, 4096, 6, 2),
        WeatherKind::Notam => (37, 20, 6095, 3, 2),
        WeatherKind::Airmet | WeatherKind::Sigmet => (26, 18, 4096, 3, 1),
        _ => unreachable!(),
    };
    if parts.len() < 3 || (kind != WeatherKind::Notam && parts.len() != 3) {
        return Err(DecodeError("weather text segment count"));
    }
    let mut metadata = Reader::new(parts[0]);
    let issue_time = PartialUtc::read(&mut metadata)?;
    metadata.take(count_offset - 3)?;
    let count = match count_size {
        1 => metadata.u8()? as usize,
        2 => metadata.u16()? as usize,
        _ => metadata.u32()? as usize,
    };
    let index = inflate(parts[1], MAX_PRODUCT_BYTES)?;
    let text = inflate(parts[2], MAX_PRODUCT_BYTES - index.len())?;
    if count.checked_mul(entry_size) != Some(index.len()) {
        return Err(DecodeError("weather index length"));
    }
    let mut reports = Vec::with_capacity(count);
    let mut copied_text = 0usize;
    for entry in index.chunks_exact(entry_size) {
        let mut pointer = Reader::new(&entry[pointer_offset..]);
        let length = pointer.u32()? as usize;
        let offset = pointer.u32()? as usize;
        copied_text = copied_text
            .checked_add(length)
            .ok_or(DecodeError("weather text overflow"))?;
        if length > max_text || copied_text > MAX_PRODUCT_BYTES {
            return Err(DecodeError("weather text limit"));
        }
        let end = offset
            .checked_add(length)
            .ok_or(DecodeError("weather pointer overflow"))?;
        let raw = text
            .get(offset..end)
            .ok_or(DecodeError("weather text pointer"))?;
        let text = std::str::from_utf8(raw)
            .map_err(|_| DecodeError("weather text is not UTF-8"))?
            .trim_matches(['\0', ' ', '\r', '\n', '\t'])
            .to_string();
        let station = if matches!(
            kind,
            WeatherKind::Metar | WeatherKind::Taf | WeatherKind::Winds
        ) {
            Some(Reader::new(entry).text(4)?)
        } else {
            None
        };
        let (notam_identifier, record_type_raw) = if kind == WeatherKind::Notam {
            (Some(Reader::new(&entry[1..]).text(15)?), Some(entry[0]))
        } else {
            (None, None)
        };
        reports.push(TextReport {
            text,
            station,
            notam_identifier,
            record_type_raw,
        });
    }
    Ok(Weather::Text {
        kind,
        issue_time,
        reports,
        additional_segments: parts[3..].iter().map(|part| part.to_vec()).collect(),
    })
}

fn decode_radar(kind: WeatherKind, parts: &[&[u8]]) -> Result<Radar> {
    if parts.len() != 3 || parts[0].len() != 26 {
        return Err(DecodeError("radar segments"));
    }
    let mut input = Reader::new(parts[0]);
    let center_latitude = input.i32()? as f64 * 180.0 / 2f64.powi(31);
    let center_longitude = input.i32()? as f64 * 180.0 / 2f64.powi(31);
    let issue_time = PartialUtc::read(&mut input)?;
    let precipitation_time = PartialUtc::read(&mut input)?;
    let reference_latitude = input.i32()? as f64 * 180.0 / 2f64.powi(31);
    let columns = input.u16()?;
    let rows = input.u16()?;
    let pixel_height_meters = input.u16()?;
    let pixel_width_meters = input.u16()?;
    let count = usize::from(columns) * usize::from(rows);
    if count == 0
        || count > 16 * 1024 * 1024
        || pixel_height_meters == 0
        || pixel_width_meters == 0
        || center_latitude.abs() > 90.0
        || reference_latitude.abs() >= 90.0
    {
        return Err(DecodeError("invalid radar geometry"));
    }
    let runs = inflate(parts[1], MAX_PRODUCT_BYTES)?;
    let offsets = inflate(parts[2], MAX_PRODUCT_BYTES - runs.len())?;
    if offsets.len() != usize::from(rows) * 4 && offsets.len() != (usize::from(rows) + 1) * 4 {
        return Err(DecodeError("radar row index size"));
    }
    let mut grid = Vec::with_capacity(count);
    for row in 0..usize::from(rows) {
        let start = Reader::new(&offsets[row * 4..]).u32()? as usize;
        let mut run = Reader::new(runs.get(start..).ok_or(DecodeError("radar row offset"))?);
        let mut filled = 0usize;
        while filled < usize::from(columns) {
            let length = run.u16()? as usize;
            let value = run.u8()?;
            if length == 0 || filled + length > usize::from(columns) {
                return Err(DecodeError("radar run width"));
            }
            filled += length;
            grid.resize(grid.len() + length, value);
        }
    }
    let radar = Radar {
        kind,
        issue_time,
        precipitation_time,
        center_latitude,
        center_longitude,
        reference_latitude,
        columns,
        rows,
        pixel_width_meters,
        pixel_height_meters,
        grid,
    };
    let (north, west) = radar.lat_lon(0.0, 0.0);
    let (south, east) = radar.lat_lon(f64::from(columns), f64::from(rows));
    if north > 90.0 || south < -90.0 || east - west > 360.0 {
        return Err(DecodeError("radar extent"));
    }
    Ok(radar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn compressed(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    pub(super) fn container(parts: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = vec![1, 1, 1, 0];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..16 {
            bytes.extend_from_slice(&(parts.get(i).map_or(0, Vec::len) as u32).to_le_bytes());
        }
        for part in parts {
            bytes.extend(part);
        }
        bytes
    }

    #[test]
    fn metar_index_resolves_text_without_inventing_position_or_date() {
        let text = b"METAR KZZZ 312355Z AUTO 00000KT 10SM CLR 10/08 A3000=";
        let mut entry = vec![0u8; 77];
        entry[..4].copy_from_slice(b"KZZZ");
        entry[69..73].copy_from_slice(&(text.len() as u32).to_le_bytes());
        let raw = container(&[vec![1, 0, 3, 1, 0], compressed(&entry), compressed(text)]);
        let Weather::Text {
            reports,
            issue_time,
            ..
        } = decode_weather(WeatherKind::Metar, &raw).unwrap()
        else {
            panic!()
        };
        assert_eq!(reports[0].station.as_deref(), Some("KZZZ"));
        assert_eq!(reports[0].text.as_bytes(), text);
        assert_eq!(
            issue_time,
            PartialUtc {
                day: 1,
                hour: 0,
                minute: 3
            }
        );
        // Container issuance is distinct from the embedded METAR observation time.
        let anchor = DateTime::parse_from_rfc3339("2026-02-01T00:05:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let observed = PartialUtc {
            day: 31,
            hour: 23,
            minute: 55,
        }
        .resolve_nearest(anchor)
        .unwrap();
        assert_eq!(observed.to_rfc3339(), "2026-01-31T23:55:00+00:00");
    }

    #[test]
    fn decompression_limits_truncation_and_trailing_bytes_are_errors() {
        let raw = compressed(&[7; 8192]);
        assert_eq!(inflate(&raw, 8192).unwrap(), vec![7; 8192]);
        assert!(inflate(&raw, 8191).is_err());
        assert!(inflate(&raw[..raw.len() - 1], 8192).is_err());
        assert!(inflate(&[raw, vec![0]].concat(), 8192).is_err());
    }

    #[test]
    fn radar_preserves_coverage_codes_and_geographic_grid() {
        let mut metadata = vec![0; 26];
        metadata[8..14].copy_from_slice(&[6, 1, 54, 6, 1, 52]);
        metadata[18..26].copy_from_slice(&[2, 0, 1, 0, 0x3c, 7, 0xda, 10]);
        let raw = container(&[
            metadata,
            compressed(&[1, 0, 24, 1, 0, 7]),
            compressed(&[0; 4]),
        ]);
        let Weather::Radar(radar) = decode_weather(WeatherKind::RegionalRadar, &raw).unwrap()
        else {
            panic!()
        };
        assert_eq!(radar.grid, [24, 7]);
        assert_eq!(radar.lat_lon(1.0, 0.5), (0.0, 0.0));
        assert!(radar.lat_lon(0.5, 0.0).0 > radar.lat_lon(0.5, 1.0).0);
    }

    #[test]
    fn empty_is_not_a_parser_error_or_a_claim_of_clear_weather() {
        assert!(matches!(
            decode_weather(WeatherKind::Metar, &[]).unwrap(),
            Weather::Empty { .. }
        ));
        assert!(decode_weather(WeatherKind::Metar, &[1, 1, 1, 0]).is_err());
    }

    #[test]
    fn future_timestamp_is_not_reinterpreted_as_last_month() {
        let anchor = DateTime::parse_from_rfc3339("2026-10-06T01:55:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let future = PartialUtc {
            day: 6,
            hour: 2,
            minute: 0,
        };
        assert_eq!(
            future.resolve_nearest(anchor),
            Some(anchor + chrono::Duration::minutes(5))
        );
    }
}

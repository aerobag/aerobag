// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Receiver radar is prepared on the ingest worker. UI sessions share immutable
//! compressed tiles, never encode images or clone national grids under their lock.

use super::{Radar, WeatherKind};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    sync::Arc,
};

pub(crate) const RESOURCE_PREFIX: &str = "core-image://receiver-radar/";
const TILE_SIZE: u32 = 256;
const MAX_HISTORY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Image {
    pub id: String,
    pub observed: DateTime<Utc>,
    issue: DateTime<Utc>,
    regional: bool,
    pub manifest: serde_json::Value,
    tiles: BTreeMap<String, Arc<[u8]>>,
    bytes: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub id: String,
    // Coarse CONUS underneath regional, with each layer's own date and palette.
    pub layers: Vec<Arc<Image>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct History {
    pub(crate) frames: Vec<Frame>,
}

impl History {
    pub fn ingest(&mut self, radar: &Radar, now: DateTime<Utc>) -> Result<bool, &'static str> {
        let regional = match radar.kind {
            WeatherKind::RegionalRadar => true,
            WeatherKind::ConusRadar => false,
            _ => return Err("not a radar product"),
        };
        let issue = radar
            .issue_time
            .resolve_nearest(now)
            .ok_or("invalid radar issue date")?;
        let observed = radar
            .precipitation_time
            .resolve_nearest(issue)
            .ok_or("invalid radar observation date")?;
        let prior = self
            .frames
            .last()
            .and_then(|frame| frame.layers.iter().find(|image| image.regional == regional));
        if prior.is_some_and(|image| issue < image.issue || observed < image.observed) {
            return Ok(false);
        }
        let mut digest = Sha256::new();
        digest.update(
            serde_json::to_vec(&(
                regional,
                issue,
                observed,
                radar.center_latitude,
                radar.center_longitude,
                radar.reference_latitude,
                radar.columns,
                radar.rows,
                radar.pixel_width_meters,
                radar.pixel_height_meters,
            ))
            .map_err(|_| "radar identity encoding")?,
        );
        digest.update(&radar.grid);
        let id: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if prior.is_some_and(|image| image.id == id) {
            return Ok(false);
        }
        if radar.columns == 0
            || radar.rows == 0
            || radar.grid.len() > 16 * 1024 * 1024
            || radar.grid.len() != usize::from(radar.columns) * usize::from(radar.rows)
        {
            return Err("invalid radar dimensions");
        }
        let (north, west) = radar.lat_lon(0.0, 0.0);
        let (next_lat, next_lon) = radar.lat_lon(1.0, 1.0);
        let (south, east) = radar.lat_lon(radar.columns.into(), radar.rows.into());
        if ![north, west, next_lat, next_lon, south, east]
            .iter()
            .all(|v| v.is_finite())
            || south >= north
            || west >= east
            || south < -90.0
            || north > 90.0
        {
            return Err("invalid radar georeference");
        }
        let width = u32::from(radar.columns);
        let height = u32::from(radar.rows);
        let mut tiles = BTreeMap::new();
        let mut bytes = 0;
        for y in 0..height.div_ceil(TILE_SIZE) {
            for x in 0..width.div_ceil(TILE_SIZE) {
                let tw = TILE_SIZE.min(width - x * TILE_SIZE);
                let th = TILE_SIZE.min(height - y * TILE_SIZE);
                let image = image::RgbaImage::from_fn(tw, th, |col, row| {
                    let index = (y * TILE_SIZE + row) * width + x * TILE_SIZE + col;
                    image::Rgba(color(radar.grid[index as usize], regional))
                });
                let mut png = Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(image)
                    .write_to(&mut png, image::ImageFormat::Png)
                    .map_err(|_| "radar tile encoding")?;
                let png = png.into_inner();
                bytes += png.len();
                if bytes > MAX_HISTORY_BYTES {
                    return Err("receiver radar memory limit");
                }
                tiles.insert(tile_url(&id, 0, x, y), Arc::from(png));
            }
        }
        let image = Arc::new(Image {
            id: id.clone(),
            issue,
            observed,
            regional,
            tiles,
            bytes,
            manifest: serde_json::json!({
                "state_id": id, "observed_at_utc": observed,
                "source_grid": {"geo_transform": [west, next_lon-west, 0.0, north, 0.0, next_lat-north]},
                "tile_size": TILE_SIZE, "tile_path_template": "tiles/res{res}/{x}/{y}.png",
                "levels": [{"res": 0, "width": width, "height": height,
                    "tile_cols": width.div_ceil(TILE_SIZE), "tile_rows": height.div_ceil(TILE_SIZE)}]
            }),
        });
        let mut layers: Vec<_> = self
            .frames
            .last()
            .into_iter()
            .flat_map(|frame| &frame.layers)
            .filter(|old| {
                old.regional != regional
                    && (now - old.observed).num_milliseconds()
                        <= crate::weather_controller::NEXRAD_MAX_DISPLAY_AGE_MS
            })
            .cloned()
            .collect();
        layers.push(image);
        layers.sort_by_key(|image| image.regional);
        // If the latest pair alone exceeds the bound, reject the whole update;
        // never publish half a frame or evict the other product silently.
        if layers.iter().map(|image| image.bytes).sum::<usize>() > MAX_HISTORY_BYTES {
            return Err("receiver radar memory limit");
        }
        let id = layers
            .iter()
            .map(|image| image.id.as_str())
            .collect::<Vec<_>>()
            .join("+");
        self.frames.push(Frame { id, layers });
        while self.frames.len() > crate::live_feeds::NEXRAD_FRAME_WINDOW_SIZE
            || self.bytes() > MAX_HISTORY_BYTES
        {
            self.frames.remove(0);
        }
        Ok(true)
    }

    fn bytes(&self) -> usize {
        let mut seen = BTreeSet::new();
        self.frames
            .iter()
            .flat_map(|frame| &frame.layers)
            .filter(|image| seen.insert(&image.id))
            .map(|image| image.bytes)
            .sum()
    }

    pub(crate) fn tile(&self, src: &str) -> Option<&[u8]> {
        self.frames
            .iter()
            .rev()
            .flat_map(|frame| &frame.layers)
            .find_map(|image| image.tiles.get(src))
            .map(AsRef::as_ref)
    }
}

pub(crate) fn tile_url(id: &str, res: u32, x: u32, y: u32) -> String {
    format!("{RESOURCE_PREFIX}{id}/tiles/res{res}/{x}/{y}.png")
}

fn color(code: u8, regional: bool) -> [u8; 4] {
    // Observed Pilot palette, not invented dBZ thresholds. Unknown codes are
    // no-data shading, never clear/no-precipitation. Original codes stay in capture.
    let argb: u32 = match code {
        0 | 8 | 16 => 0,
        1 if !regional => 0,
        1 => (-16_718_336_i32) as u32,
        2 => (-16_741_376_i32) as u32,
        3 => (-256_i32) as u32,
        4 => (-32_256_i32) as u32,
        5 => (-45_056_i32) as u32,
        6 => (-65_536_i32) as u32,
        7 => (-4_718_592_i32) as u32,
        24 if !regional => 0,
        _ => 0x30000000,
    };
    [
        (argb >> 16) as u8,
        (argb >> 8) as u8,
        argb as u8,
        (argb >> 24) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::super::test_support::radar;
    use super::*;

    #[test]
    fn source_palette_mask_dates_dedup_and_history_bounds() {
        let now = DateTime::parse_from_rfc3339("2026-10-10T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut history = History::default();
        let mut radar = radar();
        assert!(history.ingest(&radar, now).unwrap());
        assert!(!history.ingest(&radar, now).unwrap());
        let image = &history.frames[0].layers[0];
        let decoded = image::load_from_memory(image.tiles.values().next().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(decoded.get_pixel(1, 0).0, [0, 0, 0, 48]);
        assert_eq!(decoded.get_pixel(2, 0).0, [255, 0, 0, 255]);
        let transform = image.manifest["source_grid"]["geo_transform"]
            .as_array()
            .unwrap();
        assert_eq!(transform[0].as_f64().unwrap(), radar.lat_lon(0.0, 0.0).1);
        assert_eq!(transform[3].as_f64().unwrap(), radar.lat_lon(0.0, 0.0).0);
        radar.kind = WeatherKind::ConusRadar;
        history.ingest(&radar, now).unwrap();
        assert_eq!(history.frames.last().unwrap().layers.len(), 2);
        assert!(!history.frames.last().unwrap().layers[0].regional);
        for minute in 1..20 {
            radar.issue_time.minute = minute;
            history
                .ingest(&radar, now + chrono::Duration::minutes(minute.into()))
                .unwrap();
        }
        assert_eq!(
            history.frames.len(),
            crate::live_feeds::NEXRAD_FRAME_WINDOW_SIZE
        );
        radar.issue_time.minute = 0;
        assert!(!history.ingest(&radar, now).unwrap());
    }
}

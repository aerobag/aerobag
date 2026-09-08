// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use product_contracts::{
    airway_routing_chunk_key, AirwayRoutingChunk, AirwayRoutingEdge, AirwayRoutingManifest,
    AirwayRoutingNode, AirwayRoutingReference, AIRWAY_ROUTING_CHUNK_SIZE,
    AIRWAY_ROUTING_MANIFEST_KEY, AIRWAY_ROUTING_SCHEMA_VERSION,
};

#[derive(Debug)]
struct DirectionalAltitude {
    primary: Option<u32>,
    primary_direction: String,
    opposite: Option<u32>,
    opposite_direction: String,
}

impl DirectionalAltitude {
    fn read(row: &rusqlite::Row<'_>, start: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            primary: row.get(start)?,
            primary_direction: row.get(start + 1)?,
            opposite: row.get(start + 2)?,
            opposite_direction: row.get(start + 3)?,
        })
    }

    fn for_segment(&self, from: &AirwayRoutingNode, to: &AirwayRoutingNode) -> Option<u32> {
        if !self.opposite_direction.is_empty()
            && direction_matches(&self.opposite_direction, from, to)
        {
            return self.opposite;
        }
        if direction_matches(&self.primary_direction, from, to) {
            return self.primary;
        }
        None
    }
}

#[derive(Debug)]
struct Metadata {
    mea: DirectionalAltitude,
    gnss: DirectionalAltitude,
    crossing: DirectionalAltitude,
    maximum_altitude_ft: Option<u32>,
    crossing_point: String,
    excluded: bool,
    signal_gap: bool,
}

fn direction_matches(value: &str, from: &AirwayRoutingNode, to: &AirwayRoutingNode) -> bool {
    let direction = value.trim().trim_end_matches("BND").trim();
    if direction.is_empty() {
        return true;
    }
    let north = to.lat - from.lat;
    let east = ((to.lon - from.lon + 540.0) % 360.0 - 180.0)
        * ((from.lat + to.lat) * 0.5).to_radians().cos();
    let (x, y) = match direction {
        "N" => (0.0, 1.0),
        "NE" => (1.0, 1.0),
        "E" => (1.0, 0.0),
        "SE" => (1.0, -1.0),
        "S" => (0.0, -1.0),
        "SW" => (-1.0, -1.0),
        "W" => (-1.0, 0.0),
        "NW" => (-1.0, 1.0),
        _ => return false,
    };
    east * x + north * y > 0.0
}

fn distance_nm(from: &AirwayRoutingNode, to: &AirwayRoutingNode) -> f64 {
    let a = ((to.lat - from.lat).to_radians() * 0.5).sin().powi(2)
        + from.lat.to_radians().cos()
            * to.lat.to_radians().cos()
            * ((to.lon - from.lon).to_radians() * 0.5).sin().powi(2);
    3440.065 * 2.0 * a.clamp(0.0, 1.0).sqrt().asin()
}

pub(super) fn build_routing_pairs(
    connection: &rusqlite::Connection,
    branches: &BTreeMap<(String, String), Vec<serde_json::Value>>,
) -> anyhow::Result<Vec<NavKvPair>> {
    let mut statement = connection.prepare("SELECT * FROM airway_segment_metadata")?;
    let rows = statement.query_map([], |row| {
        Ok((
            (
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
            ),
            DirectionalAltitude::read(row, 3)?,
            DirectionalAltitude::read(row, 7)?,
            row.get::<_, Option<u32>>(11)?,
            DirectionalAltitude::read(row, 12)?,
            row.get::<_, String>(16)?,
            row.get::<_, String>(17)?,
        ))
    })?;
    let mut metadata = BTreeMap::new();
    for row in rows {
        let (key, mea, gnss, maximum_altitude_ft, crossing, crossing_point, flags) = row?;
        let flags: serde_json::Value = serde_json::from_str(&flags)?;
        metadata.insert(
            key,
            Metadata {
                mea,
                gnss,
                crossing,
                maximum_altitude_ft,
                crossing_point,
                excluded: flags["discontinued"] == true || flags["mea_gap"] == "U",
                signal_gap: flags["signal_gap"] == true,
            },
        );
    }
    let mut identities = BTreeMap::<String, u32>::new();
    let mut nodes = Vec::<AirwayRoutingNode>::new();
    let mut branch_nodes = BTreeMap::new();
    for (branch, points) in branches {
        let mut occurrences = Vec::new();
        for point in points {
            let nav_ref: AirwayRoutingReference = serde_json::from_value(point["nav_ref"].clone())?;
            let identity = serde_json::to_string(&nav_ref)?;
            let lat = point["position"]["lat"]
                .as_f64()
                .context("airway latitude")?;
            let lon = point["position"]["lon"]
                .as_f64()
                .context("airway longitude")?;
            anyhow::ensure!(
                lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0,
                "invalid airway coordinate"
            );
            let id = *identities.entry(identity).or_insert_with(|| {
                let id = nodes.len() as u32;
                nodes.push(AirwayRoutingNode {
                    id,
                    nav_ref,
                    lat,
                    lon,
                    edges: Vec::new(),
                });
                id
            });
            let sequence = point["sequence"].as_i64().context("airway sequence")? as i32;
            occurrences.push((id, sequence));
        }
        branch_nodes.insert(branch, occurrences);
    }
    for ((name, branch), occurrences) in branch_nodes {
        for segment in occurrences.windows(2) {
            let [(from, from_sequence), (to, to_sequence)] = segment else {
                unreachable!()
            };
            let Some(meta) = metadata.get(&(name.clone(), branch.clone(), *from_sequence)) else {
                // An ordered pair without its published segment record does not
                // establish an airway connection.
                continue;
            };
            if meta.excluded || from == to {
                continue;
            }
            let next_source_point = metadata
                .range((
                    std::ops::Bound::Excluded((name.clone(), branch.clone(), *from_sequence)),
                    std::ops::Bound::Unbounded,
                ))
                .next()
                .map(|(key, _)| key);
            if next_source_point != Some(&(name.clone(), branch.clone(), *to_sequence)) {
                // Never bridge a source point whose AWY2 position was missing.
                continue;
            }
            for (from, to, from_sequence, to_sequence) in [
                (*from, *to, *from_sequence, *to_sequence),
                (*to, *from, *to_sequence, *from_sequence),
            ] {
                let a = &nodes[from as usize];
                let b = &nodes[to as usize];
                let edge = AirwayRoutingEdge {
                    to,
                    airway_name: name.clone(),
                    branch_key: branch.clone(),
                    from_sequence,
                    to_sequence,
                    distance_nm: distance_nm(a, b),
                    mea_ft: meta.mea.for_segment(a, b),
                    gnss_mea_ft: meta.gnss.for_segment(a, b),
                    maximum_altitude_ft: meta.maximum_altitude_ft,
                    crossing_altitude_ft: meta.crossing.for_segment(a, b),
                    crossing_point: meta.crossing_point.clone(),
                    signal_gap: meta.signal_gap,
                };
                nodes[from as usize].edges.push(edge);
            }
        }
    }
    let manifest = AirwayRoutingManifest {
        schema_version: AIRWAY_ROUTING_SCHEMA_VERSION,
        chunk_count: nodes.len().div_ceil(AIRWAY_ROUTING_CHUNK_SIZE) as u32,
        node_count: nodes.len() as u32,
        edge_count: nodes.iter().map(|node| node.edges.len() as u32).sum(),
    };
    let mut pairs = vec![json_pair(
        AIRWAY_ROUTING_MANIFEST_KEY.into(),
        &serde_json::to_value(manifest)?,
        "airway routing manifest",
    )?];
    for (index, chunk) in nodes.chunks(AIRWAY_ROUTING_CHUNK_SIZE).enumerate() {
        pairs.push(json_pair(
            airway_routing_chunk_key(index as u32),
            &serde_json::to_value(AirwayRoutingChunk {
                schema_version: AIRWAY_ROUTING_SCHEMA_VERSION,
                nodes: chunk.to_vec(),
            })?,
            "airway routing chunk",
        )?);
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_graph_preserves_directional_altitudes_and_excludes_gaps() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(preprocessor_data::AIRWAY_SEGMENT_METADATA_SCHEMA)
            .unwrap();
        connection.execute_batch(r#"
          INSERT INTO airway_segment_metadata VALUES ('V2','',10,9000,'E BND',11000,'W BND',8000,'E BND',10000,'W BND',18000,12000,'E BND',NULL,'','ALPHA','{"discontinued":false,"mea_gap":"","signal_gap":true}');
          INSERT INTO airway_segment_metadata VALUES ('V2','',20,9000,'BND',NULL,'',NULL,'',NULL,'',NULL,NULL,'',NULL,'','','{"discontinued":true,"mea_gap":"N","signal_gap":false}');
          INSERT INTO airway_segment_metadata VALUES ('V4','',10,5000,'BND',NULL,'',NULL,'',NULL,'',NULL,NULL,'',NULL,'','','{"discontinued":false,"mea_gap":"U","signal_gap":false}');
        "#).unwrap();
        let point = |ident: &str, sequence, lon| serde_json::json!({"nav_ref":{"Fix":ident}, "sequence":sequence, "position":{"lat":47.0,"lon":lon}});
        let branches = BTreeMap::from([
            (
                ("V2".into(), "".into()),
                vec![
                    point("ALPHA", 10, -121.0),
                    point("BRAVO", 20, -120.0),
                    point("CHARLIE", 30, -119.0),
                ],
            ),
            (
                ("V4".into(), "".into()),
                vec![point("BRAVO", 10, -120.0), point("DELTA", 20, -119.5)],
            ),
        ]);
        let pairs = build_routing_pairs(&connection, &branches).unwrap();
        let manifest: AirwayRoutingManifest = serde_json::from_slice(&pairs[0].value).unwrap();
        let chunk: AirwayRoutingChunk = serde_json::from_slice(&pairs[1].value).unwrap();
        assert_eq!(manifest.node_count, 4, "shared fixes have one identity");
        assert_eq!(
            manifest.edge_count, 2,
            "discontinued and unusable segments are absent"
        );
        let forward = &chunk.nodes[0].edges[0];
        let reverse = &chunk.nodes[1].edges[0];
        assert_eq!((forward.mea_ft, reverse.mea_ft), (Some(9000), Some(11000)));
        assert_eq!(
            (forward.gnss_mea_ft, reverse.gnss_mea_ft),
            (Some(8000), Some(10000))
        );
        assert_eq!(forward.crossing_altitude_ft, Some(12000));
        assert_eq!(reverse.crossing_altitude_ft, None);
        assert!(forward.signal_gap);
        assert!(forward.distance_nm > 40.0 && forward.distance_nm < 42.0);
        connection.execute_batch(r#"
          INSERT INTO airway_segment_metadata SELECT name,branch_key,15,mea_ft,mea_direction,opposite_mea_ft,opposite_mea_direction,gnss_mea_ft,gnss_direction,opposite_gnss_mea_ft,opposite_gnss_direction,maximum_altitude_ft,crossing_altitude_ft,crossing_direction,opposite_crossing_altitude_ft,opposite_crossing_direction,crossing_point,flags_json FROM airway_segment_metadata WHERE name='V2' AND sequence_number=10;
        "#).unwrap();
        let interrupted = build_routing_pairs(&connection, &branches).unwrap();
        let manifest: AirwayRoutingManifest =
            serde_json::from_slice(&interrupted[0].value).unwrap();
        assert_eq!(
            manifest.edge_count, 0,
            "a missing point must not create a shortcut"
        );
    }
}

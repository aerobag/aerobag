// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    had_ops::{read_required, HadReadError},
    LatLon, NavKvQuery, NavKvStore, NavRef,
};
use product_contracts::{
    AirwayRoutingChunk, AirwayRoutingManifest, AirwayRoutingNode, AirwayRoutingReference,
    AIRWAY_ROUTING_SCHEMA_VERSION,
};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirwayNavigationMode {
    Vor,
    #[default]
    Gnss,
}

impl AirwayNavigationMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vor => "VOR",
            Self::Gnss => "GNSS",
        }
    }

    fn allows(self, edge: &product_contracts::AirwayRoutingEdge) -> bool {
        match self {
            Self::Gnss => low_airway(&edge.airway_name),
            // T routes and colored NDB airways are not VOR routes. A GNSS-only
            // altitude on a V route is also ineligible; wholly missing altitude
            // data remains an explicit unknown, not an equipment inference.
            Self::Vor => {
                edge.airway_name.starts_with('V')
                    && !edge.airway_name.ends_with('R')
                    && (edge.mea_ft.is_some() || edge.gnss_mea_ft.is_none())
            }
        }
    }

    pub fn mea(self, edge: &product_contracts::AirwayRoutingEdge) -> Option<(u32, bool)> {
        if self == Self::Gnss {
            if let Some(altitude) = edge.gnss_mea_ft {
                return Some((altitude, true));
            }
        }
        edge.mea_ft.map(|altitude| (altitude, false))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Graph {
    pub nodes: Vec<AirwayRoutingNode>,
}

pub(crate) fn node_ref(node: &AirwayRoutingNode) -> NavRef {
    match &node.nav_ref {
        AirwayRoutingReference::Airport(id) => NavRef::Airport(id.clone()),
        AirwayRoutingReference::Navaid(id) => NavRef::Navaid(id.clone()),
        AirwayRoutingReference::Fix(id) => NavRef::Fix(id.clone()),
        AirwayRoutingReference::LatLon { lat, lon } => NavRef::LatLon(LatLon {
            lat: *lat,
            lon: *lon,
        }),
    }
}

pub(crate) fn position(node: &AirwayRoutingNode) -> LatLon {
    LatLon {
        lat: node.lat,
        lon: node.lon,
    }
}

fn low_airway(name: &str) -> bool {
    matches!(
        name.as_bytes().first(),
        Some(b'V' | b'T' | b'A' | b'B' | b'G' | b'R')
    )
}

impl Graph {
    pub fn load(store: &NavKvStore) -> Result<Self, HadReadError> {
        let manifest: AirwayRoutingManifest = read_required(
            store,
            NavKvQuery::AirwayRoutingManifest,
            "airway routing manifest",
        )?;
        if manifest.schema_version != AIRWAY_ROUTING_SCHEMA_VERSION
            || manifest.node_count > 200_000
            || manifest.chunk_count > 2_000
        {
            return Err(HadReadError::Fatal(
                "Unsupported airway routing graph.".into(),
            ));
        }
        // Discover every external value page together; large adjacency chunks
        // must not turn one user action into a waterfall of page requests.
        let keys = (0..manifest.chunk_count)
            .map(product_contracts::airway_routing_chunk_key)
            .collect::<Vec<_>>();
        let pages = store
            .missing_pages_for_keys(&keys)
            .map_err(HadReadError::Fatal)?;
        if !pages.is_empty() {
            return Err(HadReadError::NeedPages(pages));
        }
        let mut nodes = Vec::new();
        let mut missing = BTreeSet::new();
        for index in 0..manifest.chunk_count {
            match read_required::<AirwayRoutingChunk>(
                store,
                NavKvQuery::AirwayRoutingChunk { index },
                "airway routing chunk",
            ) {
                Ok(chunk) => {
                    if chunk.schema_version != AIRWAY_ROUTING_SCHEMA_VERSION {
                        return Err(HadReadError::Fatal(
                            "Unsupported airway graph chunk.".into(),
                        ));
                    }
                    nodes.extend(chunk.nodes);
                }
                Err(HadReadError::NeedPages(pages)) => missing.extend(pages),
                Err(error) => return Err(error),
            }
        }
        if !missing.is_empty() {
            return Err(HadReadError::NeedPages(missing.into_iter().collect()));
        }
        if nodes.len() != manifest.node_count as usize
            || nodes.iter().map(|node| node.edges.len()).sum::<usize>()
                != manifest.edge_count as usize
        {
            return Err(HadReadError::Fatal(
                "Incomplete airway routing graph.".into(),
            ));
        }
        for (index, node) in nodes.iter().enumerate() {
            if node.id as usize != index
                || !node.lat.is_finite()
                || !node.lon.is_finite()
                || node.lat.abs() > 90.0
                || node.lon.abs() > 180.0
                || node.edges.iter().any(|edge| {
                    edge.to as usize >= nodes.len()
                        || !edge.distance_nm.is_finite()
                        || edge.distance_nm <= 0.0
                })
            {
                return Err(HadReadError::Fatal("Invalid airway routing graph.".into()));
            }
        }
        Ok(Self { nodes })
    }

    pub fn is_target(&self, id: u32, mode: AirwayNavigationMode) -> bool {
        self.nodes.get(id as usize).is_some_and(|node| {
            !matches!(node.nav_ref, AirwayRoutingReference::LatLon { .. })
                && node.edges.iter().any(|edge| mode.allows(edge))
        })
    }

    fn connections(
        &self,
        anchor: &NavRef,
        at: LatLon,
        mode: AirwayNavigationMode,
    ) -> Vec<(u32, f64)> {
        if let Some(node) = self.nodes.iter().find(|node| node_ref(node) == *anchor) {
            return vec![(node.id, 0.0)];
        }
        let mut choices = self
            .nodes
            .iter()
            .filter(|node| self.is_target(node.id, mode))
            .filter_map(|node| {
                let distance = crate::flight_leg_distance_nm(at, position(node));
                (distance <= 75.0).then_some((node.id, distance))
            })
            .collect::<Vec<_>>();
        choices.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        choices.truncate(24);
        choices
    }

    pub fn routes(
        &self,
        origin: &NavRef,
        origin_position: LatLon,
        destination: &NavRef,
        destination_position: LatLon,
        via: &[u32],
        mode: AirwayNavigationMode,
    ) -> Vec<Path> {
        if via.len() > 12 || via.iter().any(|id| !self.is_target(*id, mode)) {
            return Vec::new();
        }
        let starts = self.connections(origin, origin_position, mode);
        let ends = self.connections(destination, destination_position, mode);
        let direct = crate::flight_leg_distance_nm(origin_position, destination_position);
        self.shortest(&starts, &ends, via, mode)
            .into_iter()
            .filter(|path| {
                !via.is_empty()
                    || path.distance_nm
                        <= if direct < 15.0 {
                            direct * 2.0 + 3.0
                        } else {
                            direct * 1.75 + 20.0
                        }
            })
            .collect()
    }

    fn shortest(
        &self,
        starts: &[(u32, f64)],
        ends: &[(u32, f64)],
        via: &[u32],
        mode: AirwayNavigationMode,
    ) -> Option<Path> {
        let n = self.nodes.len();
        if n == 0 {
            return None;
        }
        // Retain the incoming airway: equal-distance arrivals at the same fix
        // can have different continuation costs. A single label per fix would
        // discard the arrival that avoids a later airway change.
        let mut indices = BTreeMap::new();
        let mut states = Vec::new();
        let mut costs = Vec::new();
        let mut previous = Vec::<Option<(usize, usize)>>::new();
        let mut queue = BinaryHeap::new();
        for &(id, direct) in starts {
            if via
                .iter()
                .position(|via_id| *via_id == id)
                .is_some_and(|index| index != 0)
            {
                continue;
            }
            let stage = usize::from(via.first() == Some(&id));
            let state = states.len();
            let key = (id, stage, None::<&str>);
            indices.insert(key, state);
            states.push(key);
            previous.push(None);
            costs.push(RouteCost {
                distance_nm: direct * 6.0,
                changes: 0,
            });
            queue.push(QueueEntry {
                cost: costs[state],
                state,
            });
        }
        let mut best = None;
        let mut best_cost = RouteCost::INFINITY;
        while let Some(QueueEntry { cost, state }) = queue.pop() {
            if cost != costs[state] {
                continue;
            }
            if cost >= best_cost {
                break;
            }
            let (node_id, stage, incoming) = states[state];
            let node = node_id as usize;
            if stage == via.len() && incoming.is_some() {
                if let Some((_, distance)) = ends.iter().find(|(id, _)| *id as usize == node) {
                    let score = RouteCost {
                        distance_nm: cost.distance_nm + distance * 6.0,
                        ..cost
                    };
                    if score < best_cost {
                        best_cost = score;
                        best = Some((state, *distance));
                    }
                }
            }
            for (edge_index, edge) in self.nodes[node].edges.iter().enumerate() {
                if !mode.allows(edge) {
                    continue;
                }
                // A required fix can only be visited at its position in the Via list.
                if via
                    .iter()
                    .position(|id| *id == edge.to)
                    .is_some_and(|index| index != stage)
                {
                    continue;
                }
                // Do not let a cheap trip out and back masquerade as an airway route
                // when both airport connectors lead to the same nearby fix.
                let mut ancestor = Some(state);
                let mut repeated = false;
                while let Some(prior) = ancestor {
                    if states[prior].0 == edge.to {
                        repeated = true;
                        break;
                    }
                    ancestor = previous[prior].map(|(state, _)| state);
                }
                if repeated {
                    continue;
                }
                let next_stage = stage + usize::from(via.get(stage) == Some(&edge.to));
                let key = (edge.to, next_stage, Some(edge.airway_name.as_str()));
                let next = *indices.entry(key).or_insert_with(|| {
                    let index = states.len();
                    states.push(key);
                    costs.push(RouteCost::INFINITY);
                    previous.push(None);
                    index
                });
                let next_cost = RouteCost {
                    distance_nm: cost.distance_nm + edge.distance_nm,
                    changes: cost.changes
                        + u32::from(incoming.is_some_and(|name| name != edge.airway_name)),
                };
                if next_cost < costs[next] {
                    costs[next] = next_cost;
                    previous[next] = Some((state, edge_index));
                    queue.push(QueueEntry {
                        cost: next_cost,
                        state: next,
                    });
                }
            }
        }
        let (mut state, end_direct) = best?;
        let mut steps = Vec::new();
        let mut seen = BTreeSet::from([states[state].0]);
        while let Some((prior, edge)) = previous[state] {
            let from = states[prior].0;
            if !seen.insert(from) {
                return None;
            }
            steps.push(Step { from, edge });
            state = prior;
        }
        steps.reverse();
        let start_direct = starts.iter().find(|(id, _)| *id == states[state].0)?.1;
        let direct_nm = start_direct + end_direct;
        let distance_nm = direct_nm
            + steps
                .iter()
                .map(|step| self.nodes[step.from as usize].edges[step.edge].distance_nm)
                .sum::<f64>();
        Some(Path {
            steps,
            direct_nm,
            distance_nm,
            score: distance_nm + 5.0 * direct_nm,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Step {
    pub from: u32,
    pub edge: usize,
}
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Path {
    pub steps: Vec<Step>,
    pub direct_nm: f64,
    pub distance_nm: f64,
    pub score: f64,
}

/// Distance is primary; airway changes only break distance ties.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RouteCost {
    distance_nm: f64,
    changes: u32,
}
impl RouteCost {
    const INFINITY: Self = Self {
        distance_nm: f64::INFINITY,
        changes: u32::MAX,
    };
}
impl Eq for RouteCost {}
impl Ord for RouteCost {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance_nm
            .total_cmp(&other.distance_nm)
            .then_with(|| self.changes.cmp(&other.changes))
    }
}
impl PartialOrd for RouteCost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct QueueEntry {
    cost: RouteCost,
    state: usize,
}
impl Eq for QueueEntry {}
impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| other.state.cmp(&self.state))
    }
}
impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
pub(crate) mod tests;

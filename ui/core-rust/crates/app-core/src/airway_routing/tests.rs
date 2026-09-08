// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use product_contracts::AirwayRoutingEdge;

pub(crate) fn graph() -> Graph {
    let mut graph = Graph {
        nodes: [
            ("START", 47.0, -122.0),
            ("YKM", 46.9, -121.0),
            ("BEEZR", 47.4, -121.3),
            ("ELN", 47.3, -120.5),
            ("END", 47.0, -120.0),
            ("ISLAND", 47.1, -120.8),
            ("CROSS", 46.9, -121.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (name, lat, lon))| AirwayRoutingNode {
            id: id as u32,
            nav_ref: AirwayRoutingReference::Fix(name.into()),
            lat,
            lon,
            edges: Vec::new(),
        })
        .collect(),
    };
    for (a, b, name, distance, mea) in [
        (0, 1, "V4", 40., Some(10000)),
        (1, 4, "V4", 40., Some(10000)),
        (0, 2, "V2", 38., Some(6000)),
        (2, 3, "V298", 36., Some(7000)),
        (3, 4, "V298", 25., None),
        (5, 6, "V9", 20., Some(4000)),
    ] {
        for (from, to) in [(a, b), (b, a)] {
            graph.nodes[from].edges.push(AirwayRoutingEdge {
                to: to as u32,
                airway_name: name.into(),
                branch_key: format!("{name}-branch"),
                from_sequence: from as i32,
                to_sequence: to as i32,
                distance_nm: distance,
                mea_ft: mea,
                gnss_mea_ft: mea.is_none().then_some(4000),
                maximum_altitude_ft: Some(18000),
                crossing_altitude_ft: if from == 2 { Some(7500) } else { None },
                crossing_point: "BEEZR".into(),
                signal_gap: false,
            });
        }
    }
    graph
}
pub(crate) fn records(graph: &Graph) -> Vec<(String, Vec<u8>)> {
    let mut records = graph
        .nodes
        .iter()
        .map(|node| {
            (
                crate::navkv::nav_kv_key_for_query(&NavKvQuery::NavRefPosition {
                    nav_ref: node_ref(node),
                    procedure_airport_id: None,
                })
                .unwrap(),
                serde_json::to_vec(&position(node)).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    records.push((
        product_contracts::AIRWAY_ROUTING_MANIFEST_KEY.into(),
        serde_json::to_vec(&AirwayRoutingManifest {
            schema_version: 1,
            chunk_count: 2,
            node_count: graph.nodes.len() as u32,
            edge_count: graph.nodes.iter().map(|node| node.edges.len() as u32).sum(),
        })
        .unwrap(),
    ));
    for (index, nodes) in [graph.nodes[..3].to_vec(), graph.nodes[3..].to_vec()]
        .into_iter()
        .enumerate()
    {
        records.push((
            product_contracts::airway_routing_chunk_key(index as u32),
            serde_json::to_vec(&AirwayRoutingChunk {
                schema_version: 1,
                nodes,
            })
            .unwrap(),
        ));
    }
    records.sort_by(|a, b| a.0.cmp(&b.0));
    records
}
pub(crate) fn store() -> NavKvStore {
    let records = records(&graph());
    crate::navkv::nav_kv_store_for_test(
        &records
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_slice()))
            .collect::<Vec<_>>(),
        4096,
    )
}
fn routes(graph: &Graph, via: &[u32]) -> Vec<Path> {
    graph.routes(
        &node_ref(&graph.nodes[0]),
        position(&graph.nodes[0]),
        &node_ref(&graph.nodes[4]),
        position(&graph.nodes[4]),
        via,
        AirwayNavigationMode::Gnss,
    )
}

#[test]
fn airway_navigation_mode_filters_routes_targets_and_airport_connections() {
    let mut graph = graph();
    for node in &mut graph.nodes {
        for edge in &mut node.edges {
            if edge.airway_name == "V4" {
                // Even a numeric conventional altitude doesn't make a T route VOR usable.
                edge.airway_name = "T261".into();
            }
            if edge.airway_name == "V298" {
                edge.mea_ft = Some(7000);
            }
        }
    }
    let search = |mode, via: &[u32]| {
        graph.routes(
            &node_ref(&graph.nodes[0]),
            position(&graph.nodes[0]),
            &node_ref(&graph.nodes[4]),
            position(&graph.nodes[4]),
            via,
            mode,
        )
    };
    let gnss = search(AirwayNavigationMode::Gnss, &[]);
    let vor = search(AirwayNavigationMode::Vor, &[]);
    assert_eq!(gnss[0].distance_nm, 80.0);
    assert_eq!(vor[0].distance_nm, 99.0);
    assert!(vor[0]
        .steps
        .iter()
        .all(|step| graph.nodes[step.from as usize].edges[step.edge]
            .airway_name
            .starts_with('V')));
    assert!(search(AirwayNavigationMode::Vor, &[1]).is_empty());
    assert!(!graph.is_target(1, AirwayNavigationMode::Vor));
    assert!(graph.is_target(1, AirwayNavigationMode::Gnss));
    let airport = NavRef::Airport("OFFGRAPH".into());
    let connections = graph.connections(
        &airport,
        position(&graph.nodes[1]),
        AirwayNavigationMode::Vor,
    );
    assert!(!connections.is_empty());
    assert!(connections.iter().all(|(id, _)| *id != 1));

    let mut edge = graph.nodes[2].edges[0].clone();
    for name in ["T261", "A1", "B1", "G1", "R1", "V12R", "J1", "Q1"] {
        edge.airway_name = name.into();
        assert!(!AirwayNavigationMode::Vor.allows(&edge), "{name}");
    }
    edge.airway_name = "V2".into();
    edge.mea_ft = None;
    edge.gnss_mea_ft = Some(4000);
    assert!(!AirwayNavigationMode::Vor.allows(&edge));
    edge.gnss_mea_ft = None;
    assert!(
        AirwayNavigationMode::Vor.allows(&edge),
        "missing metadata is not evidence of GNSS-only navigation"
    );
    assert_eq!(AirwayNavigationMode::Vor.mea(&edge), None);
}
fn names(graph: &Graph, path: &Path) -> Vec<String> {
    path.steps
        .iter()
        .map(|step| {
            graph.nodes[step.from as usize].edges[step.edge]
                .airway_name
                .clone()
        })
        .collect()
}

#[test]
fn airway_search_returns_one_route_and_honors_ordered_graph_only_constraints() {
    let graph = graph();
    let options = routes(&graph, &[]);
    assert_eq!(options.len(), 1);
    assert_eq!(names(&graph, &options[0]), ["V4", "V4"]);
    let constrained = routes(&graph, &[2, 3]);
    assert!(!constrained.is_empty());
    assert_eq!(names(&graph, &constrained[0]), ["V2", "V298", "V298"]);
    assert!(
        routes(&graph, &[5]).is_empty(),
        "no direct connection to an isolated via"
    );
    assert!(
        routes(&graph, &[3, 2]).is_empty(),
        "a via order requiring a loop is rejected"
    );
    assert!(routes(&graph, &[u32::MAX]).is_empty());
}
#[test]
fn airway_connections_penalize_direct_distance_and_never_join_coincident_fixes() {
    let graph = graph();
    let path = graph
        .shortest(
            &[(0, 0.0), (1, 9.0)],
            &[(4, 0.0)],
            &[],
            AirwayNavigationMode::Gnss,
        )
        .unwrap();
    assert_eq!(
        path.steps[0].from, 0,
        "saving 40 airway miles must not justify nine extra direct miles"
    );
    let shortcut = graph
        .shortest(
            &[(0, 0.0), (1, 5.0)],
            &[(4, 0.0)],
            &[],
            AirwayNavigationMode::Gnss,
        )
        .unwrap();
    assert_eq!(shortcut.steps[0].from, 1);
    assert_eq!(shortcut.score, 70.0);
    assert!(
        graph
            .shortest(&[(0, 0.0)], &[(6, 0.0)], &[], AirwayNavigationMode::Gnss)
            .is_none(),
        "same coordinates are not an airway junction"
    );
}
#[test]
fn airway_search_requires_a_published_edge_without_loops_or_high_airways() {
    let mut graph = graph();
    assert!(graph
        .shortest(&[(0, 0.0)], &[(0, 0.0)], &[], AirwayNavigationMode::Gnss)
        .is_none());
    let valid = graph
        .shortest(
            &[(0, 0.0)],
            &[(0, 0.0), (1, 20.0)],
            &[],
            AirwayNavigationMode::Gnss,
        )
        .unwrap();
    assert_eq!(
        valid.steps.len(),
        1,
        "a cheap return loop must not hide a valid airway route"
    );
    for node in &mut graph.nodes {
        for edge in &mut node.edges {
            edge.airway_name = "J4".into();
        }
    }
    assert!(routes(&graph, &[]).is_empty());
}
#[test]
fn airway_graph_pages_are_collected_then_validated_as_one_graph() {
    let graph = graph();
    let records = records(&graph);
    let entries = records
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_slice()))
        .collect::<Vec<_>>();
    let (mut store, pages) =
        crate::navkv::nav_kv_store_without_pages_and_pages_for_test(&entries, 4096);
    let mut rounds = 0;
    loop {
        match Graph::load(&store) {
            Ok(loaded) => {
                assert_eq!(loaded, graph);
                break;
            }
            Err(HadReadError::NeedPages(missing)) => {
                assert!(!missing.is_empty());
                rounds += 1;
                assert!(rounds <= 2);
                for id in missing {
                    store.insert_page(id, pages[id as usize].clone());
                }
            }
            Err(error) => panic!("{error:?}"),
        }
    }
}

#[test]
#[ignore = "requires a published NAV25 directory in AEROBAG_ROUTING_NAV_DIR"]
fn published_nav25_airway_routes_pae_lgu_and_rnt_lgu_via_beezr() {
    let path = std::env::var("AEROBAG_ROUTING_NAV_DIR").expect("NAV25 directory");
    let reader = nav_kv_package::NavKvDirectoryReader::new(path, "airway routing smoke test");
    let mut store = NavKvStore::new(crate::NavKvRoot::parse(&reader.read_root().unwrap()).unwrap());
    fn read<T>(
        store: &mut NavKvStore,
        reader: &nav_kv_package::NavKvDirectoryReader,
        f: impl Fn(&NavKvStore) -> Result<T, HadReadError>,
    ) -> T {
        for _ in 0..8 {
            match f(store) {
                Ok(value) => return value,
                Err(HadReadError::NeedPages(pages)) => {
                    for page in pages {
                        store.insert_page(page, reader.read_page(page).unwrap());
                    }
                }
                Err(error) => panic!("{error:?}"),
            }
        }
        panic!("NAV25 read did not converge")
    }
    let graph = read(&mut store, &reader, Graph::load);
    assert!(graph.nodes.len() > 1000);
    let beezr = graph
        .nodes
        .iter()
        .find(|node| node_ref(node) == NavRef::Fix("BEEZR".into()))
        .expect("BEEZR")
        .id;
    for origin in [
        NavRef::Navaid("PAE".into()),
        NavRef::Airport("KPAE".into()),
        NavRef::Airport("KRNT".into()),
    ] {
        let destination = NavRef::Airport("KLGU".into());
        let start = read(&mut store, &reader, |store| {
            crate::had_ops::nav_ref_position(store, &origin, None)
        });
        let end = read(&mut store, &reader, |store| {
            crate::had_ops::nav_ref_position(store, &destination, None)
        });
        for via in [vec![], vec![beezr]] {
            let paths = graph.routes(
                &origin,
                start,
                &destination,
                end,
                &via,
                AirwayNavigationMode::Gnss,
            );
            assert!(!paths.is_empty(), "no route {origin:?} to LGU via {via:?}");
            for path in &paths {
                let mut labels = Vec::new();
                for step in &path.steps {
                    let edge = &graph.nodes[step.from as usize].edges[step.edge];
                    if labels.last() != Some(&edge.airway_name) {
                        labels.push(edge.airway_name.clone());
                    }
                }
                eprintln!(
                    "{}: {:.0} NM, direct {:.1} NM",
                    labels.join(" "),
                    path.distance_nm,
                    path.direct_nm
                );
                if !via.is_empty() {
                    assert!(path.steps.iter().any(|step| step.from == beezr));
                }
            }
        }
    }
    let origin = NavRef::Airport("KRNT".into());
    let destination = NavRef::Airport("KLVM".into());
    let start = read(&mut store, &reader, |store| {
        crate::had_ops::nav_ref_position(store, &origin, None)
    });
    let end = read(&mut store, &reader, |store| {
        crate::had_ops::nav_ref_position(store, &destination, None)
    });
    let via = ["OCS", "MBW"].map(|name| {
        graph
            .nodes
            .iter()
            .find(|node| node_ref(node) == NavRef::Navaid(name.into()))
            .unwrap()
            .id
    });
    for mode in [AirwayNavigationMode::Vor, AirwayNavigationMode::Gnss] {
        let paths = graph.routes(&origin, start, &destination, end, &via, mode);
        let path = paths.first().expect("KRNT to KLVM through OCS and MBW");
        assert!(via
            .iter()
            .all(|id| path.steps.iter().any(|step| step.from == *id)));
        let mut previous = "";
        let names = path
            .steps
            .iter()
            .filter_map(|step| {
                let name = graph.nodes[step.from as usize].edges[step.edge]
                    .airway_name
                    .as_str();
                let changed = previous != name;
                previous = name;
                changed.then_some(name)
            })
            .collect::<Vec<_>>();
        let (from, edge) = path
            .steps
            .iter()
            .map(|step| (&graph.nodes[step.from as usize], step))
            .map(|(node, step)| (node, &node.edges[step.edge]))
            .find(|(node, edge)| {
                node_ref(node) == NavRef::Fix("HODNI".into()) && edge.airway_name == "V4"
            })
            .expect("V4 through HODNI");
        assert_eq!(
            node_ref(&graph.nodes[edge.to as usize]),
            NavRef::Fix("GEGME".into())
        );
        assert_eq!(
            mode.mea(edge),
            Some(match mode {
                AirwayNavigationMode::Vor => (16000, false),
                AirwayNavigationMode::Gnss => (11700, true),
            })
        );
        eprintln!(
            "{} KRNT–KLVM via OCS/MBW: {} · {:.0} NM; {:?} MEA {:?}",
            mode.label(),
            names.join(" "),
            path.distance_nm,
            node_ref(from),
            mode.mea(edge)
        );
    }
}

#[test]
fn airway_search_returns_one_route_even_when_airways_share_segments() {
    let mut graph = graph();
    let mut parallel = graph.nodes[0].edges[0].clone();
    parallel.airway_name = "T4".into();
    parallel.branch_key = "T4".into();
    graph.nodes[0].edges.push(parallel);
    assert_eq!(
        routes(&graph, &[]).len(),
        1,
        "the editor has one route, including co-published segments"
    );
}

#[test]
fn airway_ties_keep_continuity_across_pins_but_never_override_shorter_distance() {
    let mut graph = graph();
    let mut parallel = graph.nodes[0].edges[0].clone();
    parallel.airway_name = "T261".into();
    graph.nodes[0].edges.insert(0, parallel);
    // The first arrival at node 1 is T261; preserve the equally short V4
    // arrival because it can continue without a change, including through a pin.
    for via in [vec![], vec![1]] {
        assert_eq!(names(&graph, &routes(&graph, &via)[0]), ["V4", "V4"]);
    }
    graph.nodes[0].edges[0].distance_nm -= 0.001;
    assert_eq!(names(&graph, &routes(&graph, &[])[0]), ["T261", "V4"]);
}

#[test]
fn airway_ties_minimize_changes_across_different_equal_distance_paths() {
    let mut graph = graph();
    let template = graph.nodes[0].edges[0].clone();
    for node in &mut graph.nodes {
        node.edges.clear();
    }
    for (from, to, airway) in [
        (0, 1, "V1"),
        (1, 3, "V1"),
        (0, 2, "V2"),
        (2, 3, "V2"),
        (3, 4, "V2"),
    ] {
        graph.nodes[from].edges.push(AirwayRoutingEdge {
            to,
            airway_name: airway.into(),
            distance_nm: 10.0,
            ..template.clone()
        });
    }
    let path = &routes(&graph, &[])[0];
    assert_eq!(names(&graph, path), ["V2", "V2", "V2"]);
    assert_eq!(path.distance_nm, 30.0);
}

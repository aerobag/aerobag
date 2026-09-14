// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Offline sizing experiment; none of these experimental formats are published.
use had_nav_kv::{build_nav_kv_sorted, NavKvPair, NavKvRoot};
use nav_kv_package::NavKvDirectoryReader;
use product_contracts::{AirwayRoutingNode, AirwayRoutingReference};
// Frozen NAV25 input shapes for reproducing the before/after sizing experiment.
// These are not linked into the application or used as a compatibility fallback.
const AIRWAY_ROUTING_MANIFEST_KEY: &str = "airway/routing/manifest";
const AIRWAY_ROUTING_CHUNK_SIZE: usize = 128;
fn airway_routing_chunk_key(index: u32) -> String {
    format!("airway/routing/chunk/{index:05}")
}
#[derive(Serialize, Deserialize)]
struct AirwayRoutingManifest {
    schema_version: u32,
    chunk_count: u32,
    node_count: u32,
    edge_count: u32,
}
#[derive(Serialize, Deserialize)]
struct AirwayRoutingChunk {
    schema_version: u32,
    nodes: Vec<AirwayRoutingNode>,
}
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    io::{Cursor, Write},
    path::Path,
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn xz(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut child = Command::new("xz")
        .args([
            "--format=xz",
            "--check=crc64",
            "-6",
            "--stdout",
            "--threads=1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().unwrap();
    let output = std::thread::scope(|scope| {
        let writer = scope.spawn(move || input.write_all(bytes));
        let output = child.wait_with_output();
        writer.join().unwrap()?;
        output
    })?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(output.stdout)
}

fn is_low(name: &str) -> bool {
    matches!(
        name.as_bytes().first(),
        Some(b'V' | b'T' | b'A' | b'B' | b'G' | b'R')
    )
}

fn low_graph(nodes: &[AirwayRoutingNode]) -> Vec<AirwayRoutingNode> {
    let live: BTreeSet<u32> = nodes
        .iter()
        .flat_map(|n| {
            n.edges
                .iter()
                .filter(|e| is_low(&e.airway_name))
                .flat_map(|e| [n.id, e.to])
        })
        .collect();
    let remap: BTreeMap<u32, u32> = live
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i as u32))
        .collect();
    live.iter()
        .map(|id| {
            let mut n = nodes[*id as usize].clone();
            n.id = remap[id];
            n.edges.retain(|e| is_low(&e.airway_name));
            for e in &mut n.edges {
                e.to = remap[&e.to];
            }
            n
        })
        .collect()
}

// Same fields consumed today; removing unconsumed safety metadata is a sizing
// experiment, not a recommendation to discard it rather than start using it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UsedEdge {
    to: u32,
    airway_name: String,
    branch_key: String,
    from_sequence: i32,
    to_sequence: i32,
    distance_nm: f64,
    mea_ft: Option<u32>,
    gnss_mea_ft: Option<u32>,
    crossing_altitude_ft: Option<u32>,
    crossing_point: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct UsedNode {
    id: u32,
    nav_ref: AirwayRoutingReference,
    lat: f64,
    lon: f64,
    edges: Vec<UsedEdge>,
}
fn used(nodes: &[AirwayRoutingNode]) -> Vec<UsedNode> {
    nodes
        .iter()
        .map(|n| UsedNode {
            id: n.id,
            nav_ref: n.nav_ref.clone(),
            lat: n.lat,
            lon: n.lon,
            edges: n
                .edges
                .iter()
                .map(|e| UsedEdge {
                    to: e.to,
                    airway_name: e.airway_name.clone(),
                    branch_key: e.branch_key.clone(),
                    from_sequence: e.from_sequence,
                    to_sequence: e.to_sequence,
                    distance_nm: e.distance_nm,
                    mea_ft: e.mea_ft,
                    gnss_mea_ft: e.gnss_mea_ft,
                    crossing_altitude_ft: e.crossing_altitude_ft,
                    crossing_point: e.crossing_point.clone(),
                })
                .collect(),
        })
        .collect()
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Chunk<T> {
    schema_version: u32,
    nodes: Vec<T>,
}

fn encode<T: Serialize>(value: &T, binary: bool) -> Result<Vec<u8>> {
    Ok(if binary {
        postcard::to_allocvec(value)?
    } else {
        serde_json::to_vec(&serde_json::to_value(value)?)?
    })
}

fn experiment<T>(
    name: &str,
    nodes: &[T],
    edge_count: usize,
    binary: bool,
    out: &Path,
) -> Result<Value>
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug + Clone,
{
    let manifest = AirwayRoutingManifest {
        schema_version: 1,
        chunk_count: nodes.len().div_ceil(AIRWAY_ROUTING_CHUNK_SIZE) as u32,
        node_count: nodes.len() as u32,
        edge_count: edge_count as u32,
    };
    let mut pairs = vec![NavKvPair {
        key: AIRWAY_ROUTING_MANIFEST_KEY.into(),
        value: encode(&manifest, binary)?,
    }];
    for (i, nodes) in nodes.chunks(AIRWAY_ROUTING_CHUNK_SIZE).enumerate() {
        let chunk = Chunk {
            schema_version: 1,
            nodes: nodes.to_vec(),
        };
        let bytes = encode(&chunk, binary)?;
        let decoded: Chunk<T> = if binary {
            postcard::from_bytes(&bytes)?
        } else {
            serde_json::from_slice(&bytes)?
        };
        assert_eq!(chunk, decoded);
        pairs.push(NavKvPair {
            key: airway_routing_chunk_key(i as u32),
            value: bytes,
        });
    }
    let raw_value_bytes = pairs.iter().map(|p| p.value.len()).sum::<usize>();
    let built = build_nav_kv_sorted(pairs, 65536)?;
    let mut page_xz_bytes = 0;
    for page in &built.pages {
        page_xz_bytes += xz(page)?.len();
    }
    let whole = encode(
        &Chunk {
            schema_version: 1,
            nodes: nodes.to_vec(),
        },
        binary,
    )?;
    let compressed = xz(&whole)?;
    assert_eq!(
        nav_kv_package::decode_xz_if_needed(&compressed)?.as_ref(),
        whole
    );
    fs::write(
        out.join(format!(
            "{name}.{}",
            if binary { "postcard" } else { "json" }
        )),
        &whole,
    )?;
    fs::write(out.join(format!("{name}.xz")), &compressed)?;
    Ok(
        json!({"variant":name,"nodes":nodes.len(),"edges":edge_count,
        "chunk_value_bytes":raw_value_bytes,"page_count":built.pages.len(),
        "page_xz_bytes":page_xz_bytes,"whole_bytes":whole.len(),"whole_xz_bytes":compressed.len()}),
    )
}

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            "usage: airway_graph_audit <published-navdb-directory> <output-directory>".into(),
        );
    }
    let dir = Path::new(&args[0]);
    let out = Path::new(&args[1]);
    fs::create_dir_all(out)?;
    let reader = NavKvDirectoryReader::new(dir, "airway graph audit");
    let root = NavKvRoot::parse(&reader.read_root()?)?;
    let mut cache = BTreeMap::new();
    let mut read = |page| {
        Some(
            cache
                .entry(page)
                .or_insert_with(|| reader.read_page(page).expect("valid page"))
                .clone(),
        )
    };
    let manifest_bytes = root
        .extract_value(AIRWAY_ROUTING_MANIFEST_KEY, &mut read)
        .ok_or("missing manifest")?;
    let manifest: AirwayRoutingManifest = serde_json::from_slice(&manifest_bytes)?;
    let mut nodes = Vec::new();
    for i in 0..manifest.chunk_count {
        let bytes = root
            .extract_value(&airway_routing_chunk_key(i), &mut read)
            .ok_or("missing chunk")?;
        let chunk: AirwayRoutingChunk = serde_json::from_slice(&bytes)?;
        nodes.extend(chunk.nodes);
    }
    assert_eq!(nodes.len(), manifest.node_count as usize);
    let production_graph = product_contracts::AirwayRoutingGraph {
        schema_version: product_contracts::AIRWAY_ROUTING_SCHEMA_VERSION,
        nodes: nodes.clone(),
    };
    let production_raw = production_graph.encode()?;
    let production_had = had_nav_kv::build_nav_kv_sorted_with_extra_prefetch_keys(
        vec![
            NavKvPair {
                key: "chart/catalog".into(),
                value: b"[]".to_vec(),
            },
            NavKvPair {
                key: product_contracts::AIRWAY_ROUTING_GRAPH_KEY.into(),
                value: production_raw.clone(),
            },
        ],
        65536,
        &[had_nav_kv::NavKvPrefetch::Lookup(
            product_contracts::AIRWAY_ROUTING_GRAPH_KEY.into(),
        )],
    )?;
    let production_pages = production_had
        .pages
        .iter()
        .map(|page| xz(page))
        .collect::<Result<Vec<_>>>()?;
    let production_root = NavKvRoot::parse(&production_had.root_bytes)?;
    let mut production_store = had_nav_kv::NavKvStore::new(production_root);
    for page in &production_had.prefetch_pages {
        production_store.insert_page(
            *page,
            nav_kv_package::decode_xz_if_needed(&production_pages[*page as usize])?.into_owned(),
        );
    }
    let had_nav_kv::NavKvLookup::MissingPages(production_frontier) =
        production_store.get_bytes(product_contracts::AIRWAY_ROUTING_GRAPH_KEY)?
    else {
        return Err("graph must remain cold until first use".into());
    };
    for page in &production_frontier {
        production_store.insert_page(
            *page,
            nav_kv_package::decode_xz_if_needed(&production_pages[*page as usize])?.into_owned(),
        );
    }
    let had_nav_kv::NavKvLookup::Hit(bytes) =
        production_store.get_bytes(product_contracts::AIRWAY_ROUTING_GRAPH_KEY)?
    else {
        return Err("one frontier must complete the graph".into());
    };
    assert_eq!(
        product_contracts::AirwayRoutingGraph::decode(&bytes)?,
        production_graph
    );
    let edges = nodes.iter().flat_map(|n| &n.edges).collect::<Vec<_>>();
    assert_eq!(edges.len(), manifest.edge_count as usize);
    assert!(nodes.iter().enumerate().all(|(i, n)| n.id as usize == i));
    let stats = root
        .prefix_stats("airway/routing/", |p| reader.read_page(p).ok())
        .ok_or("missing prefix")?;
    let pages = stats
        .matching_leaf_pages
        .iter()
        .chain(&stats.external_value_pages)
        .copied()
        .collect::<BTreeSet<_>>();
    let compressed_bytes: u64 = pages
        .iter()
        .map(|p| {
            fs::metadata(dir.join(format!("page_{p:04}")))
                .unwrap()
                .len()
        })
        .sum();
    // An unchanged-page transport bundle: one HTTP object, identical per-page
    // compression and identifiers. This is not a new application file format.
    let mut bundle = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for p in &pages {
        let name = format!("page_{p:04}");
        bundle.start_file(
            &name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )?;
        bundle.write_all(&fs::read(dir.join(&name))?)?;
    }
    let bundle_bytes = bundle.finish()?.into_inner();
    fs::write(out.join("published-pages.zip"), &bundle_bytes)?;
    let prefetched: BTreeSet<u32> = root.prefetch_pages().iter().copied().collect();
    let prefetch_bytes: u64 = prefetched
        .iter()
        .map(|p| {
            fs::metadata(dir.join(format!("page_{p:04}")))
                .unwrap()
                .len()
        })
        .sum();
    let mut family_edges = BTreeMap::<String, usize>::new();
    for e in &edges {
        *family_edges.entry(e.airway_name[..1].into()).or_default() += 1;
    }
    let mut field_cost = BTreeMap::<String, usize>::new();
    let mut field_label_bytes = 0;
    for e in &edges {
        for (key, value) in serde_json::to_value(e)?.as_object().unwrap() {
            // Marginal JSON bytes including field name, colon, and a comma.
            *field_cost.entry(key.clone()).or_default() +=
                key.len() + 4 + serde_json::to_vec(value)?.len();
            field_label_bytes += key.len() + 4;
        }
    }
    let mut refs = BTreeMap::<String, usize>::new();
    for n in &nodes {
        let kind = match n.nav_ref {
            AirwayRoutingReference::Airport(_) => "airport",
            AirwayRoutingReference::Navaid(_) => "navaid",
            AirwayRoutingReference::Fix(_) => "fix",
            AirwayRoutingReference::LatLon { .. } => "coordinate",
        };
        *refs.entry(kind.into()).or_default() += 1;
    }
    let low = low_graph(&nodes);
    let low_edges = low.iter().map(|n| n.edges.len()).sum::<usize>();
    // Keep all node IDs and exact-anchor behavior for this intermediate row.
    // Dropping orphan nodes can change Graph::connections for high-only fixes.
    let mut low_edges_only = nodes.clone();
    for n in &mut low_edges_only {
        n.edges.retain(|e| is_low(&e.airway_name));
    }
    let mut matrix = Vec::new();
    for (label, graph) in [
        ("full", &nodes),
        ("low-edges", &low_edges_only),
        ("low", &low),
    ] {
        let count = graph.iter().map(|n| n.edges.len()).sum::<usize>();
        for binary in [false, true] {
            let format = if binary { "postcard" } else { "json" };
            let row = experiment(&format!("{label}-{format}"), graph, count, binary, out)?;
            eprintln!("{row}");
            matrix.push(row);
            let row = experiment(
                &format!("{label}-used-{format}"),
                &used(graph),
                count,
                binary,
                out,
            )?;
            eprintln!("{row}");
            matrix.push(row);
        }
    }
    let report = json!({"source":dir, "manifest":manifest,"published":{
    "value_bytes":stats.value_bytes,"value_pages":stats.external_value_pages,
    "leaf_pages":stats.matching_leaf_pages,"compressed_bytes":compressed_bytes,
    "unchanged_pages_bundle_bytes":bundle_bytes.len(),
    "pages_already_prefetched":pages.intersection(&prefetched).collect::<Vec<_>>(),
    "startup_prefetch_page_count":prefetched.len(),"startup_prefetch_bytes":prefetch_bytes,
    "total_navdb_page_count":root.page_count()},
    "low_graph":{"nodes":low.len(),"edges":low_edges},"family_edges":family_edges,
    "node_reference_kinds":refs,"edge_json_field_bytes":field_cost,"edge_json_label_and_delimiter_bytes":field_label_bytes,
    "unused_nondefault":{"maximum_altitude_ft":edges.iter().filter(|e|e.maximum_altitude_ft.is_some()).count(),
    "signal_gap":edges.iter().filter(|e|e.signal_gap).count()},
    "strings":{"airway_unique":edges.iter().map(|e|&e.airway_name).collect::<BTreeSet<_>>().len(),
    "branch_unique":edges.iter().map(|e|&e.branch_key).collect::<BTreeSet<_>>().len(),
    "branch_lengths":edges.iter().map(|e|e.branch_key.len()).collect::<BTreeSet<_>>()},
    "matrix":matrix, "selected_nav26_had_value": {
        "key": product_contracts::AIRWAY_ROUTING_GRAPH_KEY,
        "postcard_bytes": production_raw.len(),
        "page_count": production_frontier.len(),
        "xz_bytes": production_frontier.iter().map(|p| production_pages[*p as usize].len()).sum::<usize>(),
        "lookup_prefetch_pages": production_had.prefetch_pages,
        "lookup_prefetch_bytes": production_had.prefetch_pages.iter().map(|p| production_pages[*p as usize].len()).sum::<usize>()
    }});
    fs::write(out.join("report.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

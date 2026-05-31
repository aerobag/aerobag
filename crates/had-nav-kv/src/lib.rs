use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};

pub const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
// Low-level binary root/page storage format. This is part of the public nav-db
// product contract: changing it must also bump product_contracts::NAV_DB_CONTRACT_ID
// to NAV{NAVKV_STORAGE_FORMAT}, because clients use that token before fetch.
pub const NAVKV_STORAGE_FORMAT: u32 = 4;
pub const VERSION: u32 = NAVKV_STORAGE_FORMAT;
pub const HEADER_LEN: usize = 64;
const PREFETCH_COUNT_OFFSET: usize = 56;
const NODE_KIND_LEAF: u32 = 1;
const NODE_KIND_INTERNAL: u32 = 2;
const LEAF_HEADER_LEN: usize = 12;
const INTERNAL_HEADER_LEN: usize = 12;
const LEAF_ENTRY_FIXED_LEN: usize = 16;
const INLINE_VALUE_MAX_LEN: usize = 4096;
const NO_PAGE: u32 = u32::MAX;
const VALUE_KIND_EXTERNAL: u32 = 0;
const VALUE_KIND_INLINE: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvPair {
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvBuildOutput {
    pub root_bytes: Vec<u8>,
    pub pages: Vec<Vec<u8>>,
    pub page_size: u32,
    pub logical_bytes_len: u32,
    pub value_bytes_len: u32,
    pub prefetch_pages: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvRoot {
    entry_count: u32,
    page_size: u32,
    root_page: u32,
    page_count: u32,
    value_page_start: u32,
    value_bytes_len: u32,
    prefetch_pages: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvPrefixStats {
    pub key_count: usize,
    pub key_bytes: usize,
    pub value_bytes: usize,
    pub inline_value_count: usize,
    pub external_value_count: usize,
    pub matching_leaf_pages: Vec<u32>,
    pub external_value_pages: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavKvLookup {
    Hit(Vec<u8>),
    MissingKey,
    MissingPages(Vec<u32>),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NavKvPageProbeStats {
    pub keys: usize,
    pub node_page_hits: usize,
    pub node_page_misses: usize,
    pub leaf_entries_scanned: usize,
    pub inline_values: usize,
    pub external_values: usize,
    pub value_page_hits: usize,
    pub value_page_misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvDeltaEntry {
    pub key: String,
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvDelta {
    pub entries: Vec<NavKvDeltaEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavKvStore {
    root: NavKvRoot,
    pages: HashMap<u32, Vec<u8>>,
}

#[derive(Debug, Clone)]
struct LeafEntry {
    key: Vec<u8>,
    value: LeafEntryValue,
}

#[derive(Debug, Clone)]
enum LeafEntryValue {
    Inline(Vec<u8>),
    External { offset: u32, len: u32 },
}

#[derive(Debug, Clone)]
struct NodeSummary {
    page: u32,
    first_key: Vec<u8>,
}

#[derive(Debug, Clone)]
struct LeafLookup {
    key: Vec<u8>,
    value: LeafEntryValue,
}

#[derive(Debug, Clone)]
struct LeafNode {
    next_leaf: Option<u32>,
    entries: Vec<LeafLookup>,
}

#[derive(Debug, Clone)]
struct InternalNode {
    children: Vec<u32>,
    pivots: Vec<Vec<u8>>,
}

enum Node {
    Leaf(LeafNode),
    Internal(InternalNode),
}

enum LeafEntryValueRef<'a> {
    Inline(&'a [u8]),
    External { offset: u32, len: u32 },
}

pub fn build_nav_kv_sorted(
    mut pairs: Vec<NavKvPair>,
    page_size: u32,
) -> Result<NavKvBuildOutput, String> {
    pairs.sort_by(|left, right| left.key.as_bytes().cmp(right.key.as_bytes()));
    build_nav_kv_strict(pairs, page_size)
}

pub fn build_nav_kv_strict(
    pairs: Vec<NavKvPair>,
    page_size: u32,
) -> Result<NavKvBuildOutput, String> {
    validate_pairs(&pairs, page_size)?;
    let page_size_usize = usize::try_from(page_size)
        .map_err(|_| "nav_kv page size does not fit usize".to_string())?;

    let mut value_bytes = Vec::new();
    let mut leaf_entries = Vec::with_capacity(pairs.len());
    for pair in &pairs {
        let value_len = u32::try_from(pair.value.len())
            .map_err(|_| "nav_kv value length exceeds u32".to_string())?;
        let value = if pair.value.len() <= INLINE_VALUE_MAX_LEN {
            LeafEntryValue::Inline(pair.value.clone())
        } else {
            let value_offset = u32::try_from(value_bytes.len())
                .map_err(|_| "nav_kv value bytes exceed u32".to_string())?;
            value_bytes.extend_from_slice(&pair.value);
            LeafEntryValue::External {
                offset: value_offset,
                len: value_len,
            }
        };
        leaf_entries.push(LeafEntry {
            key: pair.key.as_bytes().to_vec(),
            value,
        });
    }
    let value_bytes_len = u32::try_from(value_bytes.len())
        .map_err(|_| "nav_kv value bytes exceed u32".to_string())?;

    let mut pages = Vec::new();
    let mut level = build_leaf_pages(&leaf_entries, page_size_usize, &mut pages)?;
    while level.len() > 1 {
        level = build_internal_level(&level, page_size_usize, &mut pages)?;
    }
    let root_page = level
        .first()
        .map(|summary| summary.page)
        .ok_or_else(|| "nav_kv requires at least one root page".to_string())?;
    let value_page_start =
        u32::try_from(pages.len()).map_err(|_| "nav_kv page count exceeds u32".to_string())?;
    pages.extend(
        value_bytes
            .chunks(page_size_usize)
            .map(|chunk| chunk.to_vec()),
    );
    let page_count =
        u32::try_from(pages.len()).map_err(|_| "nav_kv page count exceeds u32".to_string())?;
    let logical_bytes_len = page_count
        .checked_mul(page_size)
        .ok_or_else(|| "nav_kv logical length exceeds u32".to_string())?;
    let root_without_prefetch = build_root_bytes(
        u32::try_from(pairs.len()).map_err(|_| "nav_kv entry count exceeds u32".to_string())?,
        page_size,
        root_page,
        page_count,
        value_page_start,
        value_bytes_len,
        &[],
    )?;
    let root = NavKvRoot::parse(&root_without_prefetch)?;
    let prefetch_pages = startup_prefetch_pages(&root, &pages)?;
    let root_bytes = build_root_bytes(
        u32::try_from(pairs.len()).map_err(|_| "nav_kv entry count exceeds u32".to_string())?,
        page_size,
        root_page,
        page_count,
        value_page_start,
        value_bytes_len,
        &prefetch_pages,
    )?;

    Ok(NavKvBuildOutput {
        root_bytes,
        pages,
        page_size,
        logical_bytes_len,
        value_bytes_len,
        prefetch_pages,
    })
}

pub fn nav_kv_canonical_sha256_from_pairs(pairs: &[NavKvPair]) -> String {
    debug_assert!(validate_sorted_unique_pairs(pairs).is_ok());
    let mut hasher = Sha256::new();
    for pair in pairs {
        hasher.update(pair.key.as_bytes());
        hasher.update([0]);
        hasher.update((pair.value.len() as u64).to_le_bytes());
        hasher.update([0]);
        hasher.update(&pair.value);
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn build_nav_kv_delta(from: &[NavKvPair], to: &[NavKvPair]) -> Result<NavKvDelta, String> {
    validate_sorted_unique_pairs(from)?;
    validate_sorted_unique_pairs(to)?;
    let mut entries = Vec::new();
    let mut left = 0;
    let mut right = 0;
    while left < from.len() || right < to.len() {
        match (from.get(left), to.get(right)) {
            (Some(from_pair), Some(to_pair)) => match from_pair.key.cmp(&to_pair.key) {
                Ordering::Less => {
                    entries.push(NavKvDeltaEntry {
                        key: from_pair.key.clone(),
                        value: None,
                    });
                    left += 1;
                }
                Ordering::Greater => {
                    entries.push(NavKvDeltaEntry {
                        key: to_pair.key.clone(),
                        value: Some(to_pair.value.clone()),
                    });
                    right += 1;
                }
                Ordering::Equal => {
                    if from_pair.value != to_pair.value {
                        entries.push(NavKvDeltaEntry {
                            key: to_pair.key.clone(),
                            value: Some(to_pair.value.clone()),
                        });
                    }
                    left += 1;
                    right += 1;
                }
            },
            (Some(from_pair), None) => {
                entries.push(NavKvDeltaEntry {
                    key: from_pair.key.clone(),
                    value: None,
                });
                left += 1;
            }
            (None, Some(to_pair)) => {
                entries.push(NavKvDeltaEntry {
                    key: to_pair.key.clone(),
                    value: Some(to_pair.value.clone()),
                });
                right += 1;
            }
            (None, None) => break,
        }
    }
    Ok(NavKvDelta { entries })
}

pub fn apply_nav_kv_delta(
    from: &[NavKvPair],
    delta: &NavKvDelta,
) -> Result<Vec<NavKvPair>, String> {
    validate_sorted_unique_pairs(from)?;
    validate_sorted_unique_delta(delta)?;
    let mut out = Vec::new();
    let mut left = 0;
    let mut right = 0;
    while left < from.len() || right < delta.entries.len() {
        match (from.get(left), delta.entries.get(right)) {
            (Some(pair), Some(entry)) => match pair.key.cmp(&entry.key) {
                Ordering::Less => {
                    out.push(pair.clone());
                    left += 1;
                }
                Ordering::Greater => {
                    if let Some(value) = &entry.value {
                        out.push(NavKvPair {
                            key: entry.key.clone(),
                            value: value.clone(),
                        });
                    }
                    right += 1;
                }
                Ordering::Equal => {
                    if let Some(value) = &entry.value {
                        out.push(NavKvPair {
                            key: entry.key.clone(),
                            value: value.clone(),
                        });
                    }
                    left += 1;
                    right += 1;
                }
            },
            (Some(pair), None) => {
                out.push(pair.clone());
                left += 1;
            }
            (None, Some(entry)) => {
                if let Some(value) = &entry.value {
                    out.push(NavKvPair {
                        key: entry.key.clone(),
                        value: value.clone(),
                    });
                }
                right += 1;
            }
            (None, None) => break,
        }
    }
    Ok(out)
}

impl NavKvRoot {
    pub fn parse(root_bytes: &[u8]) -> Result<Self, String> {
        if root_bytes.len() < HEADER_LEN {
            return Err("nav_kv root is shorter than header".to_string());
        }
        if &root_bytes[0..16] != MAGIC {
            return Err("nav_kv root has invalid magic".to_string());
        }
        let version = read_u32(root_bytes, 16)?;
        if version != NAVKV_STORAGE_FORMAT {
            return Err(format!("unsupported nav_kv version {version}"));
        }
        let entry_count = read_u32(root_bytes, 20)?;
        let page_size = read_u32(root_bytes, 24)?;
        let root_page = read_u32(root_bytes, 28)?;
        let page_count = read_u32(root_bytes, 32)?;
        let value_page_start = read_u32(root_bytes, 36)?;
        let value_bytes_len = read_u32(root_bytes, 40)?;
        let prefetch_count = read_u32(root_bytes, PREFETCH_COUNT_OFFSET)?;
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        if page_count == 0 {
            return Err("nav_kv page_count must be non-zero".to_string());
        }
        if root_page >= page_count {
            return Err("nav_kv root_page exceeds page count".to_string());
        }
        if value_page_start > page_count {
            return Err("nav_kv value_page_start exceeds page count".to_string());
        }
        let value_page_count = value_bytes_len.div_ceil(page_size);
        if value_page_start + value_page_count > page_count {
            return Err("nav_kv value bytes exceed value pages".to_string());
        }
        let expected_root_len = HEADER_LEN
            .checked_add(
                usize::try_from(prefetch_count)
                    .map_err(|_| "nav_kv prefetch count does not fit usize".to_string())?
                    .checked_mul(4)
                    .ok_or_else(|| "nav_kv prefetch root length overflows".to_string())?,
            )
            .ok_or_else(|| "nav_kv prefetch root length overflows".to_string())?;
        if root_bytes.len() != expected_root_len {
            return Err("nav_kv root length does not match version metadata".to_string());
        }
        let mut prefetch_pages = Vec::with_capacity(prefetch_count as usize);
        let mut previous_prefetch_page = None;
        for index in 0..prefetch_count as usize {
            let page = read_u32(root_bytes, HEADER_LEN + index * 4)?;
            if page >= page_count {
                return Err("nav_kv prefetch page exceeds page count".to_string());
            }
            if let Some(previous) = previous_prefetch_page {
                if page <= previous {
                    return Err("nav_kv prefetch pages must be sorted and unique".to_string());
                }
            }
            previous_prefetch_page = Some(page);
            prefetch_pages.push(page);
        }
        Ok(Self {
            entry_count,
            page_size,
            root_page,
            page_count,
            value_page_start,
            value_bytes_len,
            prefetch_pages,
        })
    }

    pub fn len(&self) -> usize {
        self.entry_count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn page_count(&self) -> u32 {
        self.page_count
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    pub fn logical_bytes_len(&self) -> u32 {
        self.page_count.saturating_mul(self.page_size)
    }

    pub fn value_bytes_len(&self) -> u32 {
        self.value_bytes_len
    }

    pub fn prefetch_pages(&self) -> &[u32] {
        &self.prefetch_pages
    }

    pub fn get_value_range<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        key: &str,
        mut page_provider: P,
    ) -> Option<(u32, u32)> {
        match self
            .find_leaf_entry(key.as_bytes(), &mut page_provider)?
            .value
        {
            LeafEntryValue::External { offset, len } => Some((offset, offset + len)),
            LeafEntryValue::Inline(_) => Some((0, 0)),
        }
    }

    pub fn prefix_keys<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        prefix: &str,
        mut page_provider: P,
    ) -> Option<Vec<String>> {
        let mut leaf_page = self.find_leaf_page_for_key(prefix.as_bytes(), &mut page_provider)?;
        let mut out = Vec::new();
        loop {
            let leaf = self.read_leaf(leaf_page, &mut page_provider)?;
            for entry in &leaf.entries {
                if entry.key.as_slice() < prefix.as_bytes() {
                    continue;
                }
                if !entry.key.starts_with(prefix.as_bytes()) {
                    return Some(out);
                }
                out.push(String::from_utf8_lossy(&entry.key).into_owned());
            }
            match leaf.next_leaf {
                Some(next) => leaf_page = next,
                None => return Some(out),
            }
        }
    }

    pub fn prefix_stats<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        prefix: &str,
        mut page_provider: P,
    ) -> Option<NavKvPrefixStats> {
        let prefix = prefix.as_bytes();
        let mut leaf_page = self.find_leaf_page_for_key(prefix, &mut page_provider)?;
        let mut matching_leaf_pages = BTreeSet::new();
        let mut external_value_pages = BTreeSet::new();
        let mut stats = NavKvPrefixStats {
            key_count: 0,
            key_bytes: 0,
            value_bytes: 0,
            inline_value_count: 0,
            external_value_count: 0,
            matching_leaf_pages: Vec::new(),
            external_value_pages: Vec::new(),
        };

        loop {
            let leaf = self.read_leaf(leaf_page, &mut page_provider)?;
            let mut matched_leaf = false;
            for entry in &leaf.entries {
                if entry.key.as_slice() < prefix {
                    continue;
                }
                if !entry.key.starts_with(prefix) {
                    if matched_leaf {
                        matching_leaf_pages.insert(leaf_page);
                    }
                    stats.matching_leaf_pages = matching_leaf_pages.into_iter().collect();
                    stats.external_value_pages = external_value_pages.into_iter().collect();
                    return Some(stats);
                }
                matched_leaf = true;
                stats.key_count += 1;
                stats.key_bytes += entry.key.len();
                match &entry.value {
                    LeafEntryValue::Inline(bytes) => {
                        stats.inline_value_count += 1;
                        stats.value_bytes += bytes.len();
                    }
                    LeafEntryValue::External { offset, len } => {
                        stats.external_value_count += 1;
                        stats.value_bytes += *len as usize;
                        if *len > 0 {
                            let start_page = *offset / self.page_size;
                            let end_page = (*offset + *len - 1) / self.page_size;
                            for page in start_page..=end_page {
                                external_value_pages.insert(self.value_page_start + page);
                            }
                        }
                    }
                }
            }
            if matched_leaf {
                matching_leaf_pages.insert(leaf_page);
            }
            match leaf.next_leaf {
                Some(next) => leaf_page = next,
                None => {
                    stats.matching_leaf_pages = matching_leaf_pages.into_iter().collect();
                    stats.external_value_pages = external_value_pages.into_iter().collect();
                    return Some(stats);
                }
            }
        }
    }

    pub fn pairs<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        mut page_provider: P,
    ) -> Option<Vec<NavKvPair>> {
        let mut leaf_page = self.first_leaf_page(&mut page_provider)?;
        let mut out = Vec::with_capacity(self.len());
        loop {
            let leaf = self.read_leaf(leaf_page, &mut page_provider)?;
            for entry in leaf.entries {
                let value = match entry.value {
                    LeafEntryValue::Inline(bytes) => bytes,
                    LeafEntryValue::External { offset, len } => {
                        self.read_external_value(offset, len, &mut page_provider)?
                    }
                };
                out.push(NavKvPair {
                    key: String::from_utf8_lossy(&entry.key).into_owned(),
                    value,
                });
            }
            match leaf.next_leaf {
                Some(next) => leaf_page = next,
                None => return Some(out),
            }
        }
    }

    pub fn canonical_sha256<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        mut page_provider: P,
    ) -> Option<String> {
        let mut hasher = Sha256::new();
        for pair in self.pairs(&mut page_provider)? {
            hasher.update(pair.key.as_bytes());
            hasher.update([0]);
            hasher.update((pair.value.len() as u64).to_le_bytes());
            hasher.update([0]);
            hasher.update(pair.value);
            hasher.update([0xff]);
        }
        Some(format!("{:x}", hasher.finalize()))
    }

    pub fn extract_value<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        key: &str,
        mut page_provider: P,
    ) -> Option<Vec<u8>> {
        let entry = self.find_leaf_entry(key.as_bytes(), &mut page_provider)?;
        match entry.value {
            LeafEntryValue::Inline(bytes) => Some(bytes),
            LeafEntryValue::External { offset, len } => {
                if len == 0 {
                    return None;
                }
                self.read_external_value(offset, len, page_provider)
            }
        }
    }

    fn first_leaf_page<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        mut page_provider: P,
    ) -> Option<u32> {
        let mut page_index = self.root_page;
        loop {
            match self.read_node(page_index, &mut page_provider)? {
                Node::Leaf(_) => return Some(page_index),
                Node::Internal(node) => page_index = *node.children.first()?,
            }
        }
    }

    fn find_leaf_entry<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        key: &[u8],
        mut page_provider: P,
    ) -> Option<LeafLookup> {
        let leaf_page = self.find_leaf_page_for_key(key, &mut page_provider)?;
        let leaf = self.read_leaf(leaf_page, &mut page_provider)?;
        leaf.entries
            .binary_search_by(|entry| entry.key.as_slice().cmp(key))
            .ok()
            .and_then(|index| leaf.entries.get(index).cloned())
    }

    fn find_leaf_page_for_key<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        key: &[u8],
        mut page_provider: P,
    ) -> Option<u32> {
        let mut page_index = self.root_page;
        loop {
            match self.read_node(page_index, &mut page_provider)? {
                Node::Leaf(_) => return Some(page_index),
                Node::Internal(node) => {
                    let child_index = node.pivots.partition_point(|pivot| pivot.as_slice() <= key);
                    page_index = *node.children.get(child_index)?;
                }
            }
        }
    }

    fn read_node<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        page: u32,
        mut page_provider: P,
    ) -> Option<Node> {
        if page >= self.value_page_start {
            return None;
        }
        let bytes = page_provider(page)?;
        match read_u32(&bytes, 0).ok()? {
            NODE_KIND_LEAF => Some(Node::Leaf(parse_leaf_node(&bytes).ok()?)),
            NODE_KIND_INTERNAL => Some(Node::Internal(parse_internal_node(&bytes).ok()?)),
            _ => None,
        }
    }

    fn read_leaf<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        page: u32,
        mut page_provider: P,
    ) -> Option<LeafNode> {
        match self.read_node(page, &mut page_provider)? {
            Node::Leaf(leaf) => Some(leaf),
            Node::Internal(_) => None,
        }
    }

    fn read_external_value<P: FnMut(u32) -> Option<Vec<u8>>>(
        &self,
        start: u32,
        len: u32,
        mut page_provider: P,
    ) -> Option<Vec<u8>> {
        if len == 0 {
            return Some(Vec::new());
        }
        let end = start.checked_add(len)?;
        if end > self.value_bytes_len {
            return None;
        }
        let start_page = start / self.page_size;
        let end_page = (end - 1) / self.page_size;
        let mut out = Vec::with_capacity(len as usize);
        for value_page in start_page..=end_page {
            let page_index = self.value_page_start + value_page;
            let page = page_provider(page_index)?;
            let page_start = value_page * self.page_size;
            let slice_start = start.saturating_sub(page_start) as usize;
            let slice_end = (end.min(page_start + self.page_size) - page_start) as usize;
            if slice_end > page.len() || slice_start > slice_end {
                return None;
            }
            out.extend_from_slice(&page[slice_start..slice_end]);
        }
        Some(out)
    }
}

impl NavKvStore {
    pub fn new(root: NavKvRoot) -> Self {
        Self {
            root,
            pages: HashMap::new(),
        }
    }

    pub fn root(&self) -> &NavKvRoot {
        &self.root
    }

    pub fn insert_page(&mut self, page_index: u32, bytes: Vec<u8>) {
        self.pages.insert(page_index, bytes);
    }

    pub fn missing_prefetch_pages(&self) -> Vec<u32> {
        self.root
            .prefetch_pages()
            .iter()
            .copied()
            .filter(|page| !self.pages.contains_key(page))
            .collect()
    }

    pub fn get_bytes(&self, key: &str) -> Result<NavKvLookup, String> {
        let mut stats = NavKvPageProbeStats::default();
        let mut missing_pages = BTreeSet::new();
        let entry = self.find_leaf_entry_ref(key.as_bytes(), &mut missing_pages, &mut stats)?;
        let Some(entry) = entry else {
            let pages = missing_pages.into_iter().collect::<Vec<_>>();
            return if pages.is_empty() {
                Ok(NavKvLookup::MissingKey)
            } else {
                Ok(NavKvLookup::MissingPages(pages))
            };
        };
        match entry {
            LeafEntryValueRef::Inline(bytes) => Ok(NavKvLookup::Hit(bytes.to_vec())),
            LeafEntryValueRef::External { offset, len } => {
                let mut missing_pages = BTreeSet::new();
                match self.read_external_value_borrowed(offset, len, &mut missing_pages, &mut stats)
                {
                    Some(bytes) => Ok(NavKvLookup::Hit(bytes)),
                    None => {
                        let pages = missing_pages.into_iter().collect::<Vec<_>>();
                        if pages.is_empty() {
                            Err("nav_kv failed to read external value".to_string())
                        } else {
                            Ok(NavKvLookup::MissingPages(pages))
                        }
                    }
                }
            }
        }
    }

    pub fn missing_pages_for_keys(&self, keys: &[String]) -> Result<Vec<u32>, String> {
        self.missing_pages_for_keys_with_stats(keys)
            .map(|(pages, _stats)| pages)
    }

    pub fn missing_pages_for_keys_with_stats(
        &self,
        keys: &[String],
    ) -> Result<(Vec<u32>, NavKvPageProbeStats), String> {
        let mut stats = NavKvPageProbeStats::default();
        let mut pages = BTreeSet::new();
        for key in keys {
            stats.keys += 1;
            pages.extend(self.missing_pages_for_key(key, &mut stats)?);
        }
        Ok((pages.into_iter().collect(), stats))
    }

    pub fn keys_with_prefix(&self, prefix: &str) -> Vec<String> {
        match self.keys_with_prefix_lookup(prefix) {
            Ok(NavKvLookup::Hit(bytes)) => String::from_utf8_lossy(&bytes)
                .lines()
                .map(str::to_string)
                .collect(),
            Ok(NavKvLookup::MissingKey | NavKvLookup::MissingPages(_)) | Err(_) => Vec::new(),
        }
    }

    pub fn keys_with_prefix_lookup(&self, prefix: &str) -> Result<NavKvLookup, String> {
        let mut missing_pages = BTreeSet::new();
        let keys = {
            let mut provider = |page| self.page_bytes_or_record_missing(page, &mut missing_pages);
            self.root.prefix_keys(prefix, &mut provider)
        };
        match keys {
            Some(keys) => Ok(NavKvLookup::Hit(keys.join("\n").into_bytes())),
            None => {
                let pages = missing_pages.into_iter().collect::<Vec<_>>();
                if pages.is_empty() {
                    Err("nav_kv failed to scan prefix".to_string())
                } else {
                    Ok(NavKvLookup::MissingPages(pages))
                }
            }
        }
    }

    fn missing_pages_for_key(
        &self,
        key: &str,
        stats: &mut NavKvPageProbeStats,
    ) -> Result<Vec<u32>, String> {
        let mut missing_pages = BTreeSet::new();
        let entry = self.find_leaf_entry_ref(key.as_bytes(), &mut missing_pages, stats)?;
        let Some(entry) = entry else {
            return Ok(missing_pages.into_iter().collect());
        };
        if let LeafEntryValueRef::External { offset, len } = entry {
            self.record_missing_external_value_pages(offset, len, &mut missing_pages, stats)?;
        }
        Ok(missing_pages.into_iter().collect())
    }

    fn find_leaf_entry_ref<'a>(
        &'a self,
        key: &[u8],
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Result<Option<LeafEntryValueRef<'a>>, String> {
        let Some(leaf_page) = self.find_leaf_page_for_key_ref(key, missing_pages, stats)? else {
            return Ok(None);
        };
        let Some(leaf_bytes) =
            self.node_page_bytes_or_record_missing(leaf_page, missing_pages, stats)
        else {
            return Ok(None);
        };
        leaf_entry_value_ref_for_key(leaf_bytes, key, stats)
    }

    fn find_leaf_page_for_key_ref(
        &self,
        key: &[u8],
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Result<Option<u32>, String> {
        let mut page_index = self.root.root_page;
        loop {
            let Some(bytes) =
                self.node_page_bytes_or_record_missing(page_index, missing_pages, stats)
            else {
                return Ok(None);
            };
            match read_u32(bytes, 0)? {
                NODE_KIND_LEAF => return Ok(Some(page_index)),
                NODE_KIND_INTERNAL => {
                    page_index = internal_child_for_key_ref(bytes, key)?;
                }
                _ => return Err("nav_kv node has invalid kind".to_string()),
            }
        }
    }

    fn read_external_value_borrowed(
        &self,
        start: u32,
        len: u32,
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Option<Vec<u8>> {
        if len == 0 {
            return Some(Vec::new());
        }
        let end = start.checked_add(len)?;
        if end > self.root.value_bytes_len {
            return None;
        }
        let start_page = start / self.root.page_size;
        let end_page = (end - 1) / self.root.page_size;
        let mut out = Vec::with_capacity(len as usize);
        for value_page in start_page..=end_page {
            let page_index = self.root.value_page_start + value_page;
            let page = self.value_page_bytes_or_record_missing(page_index, missing_pages, stats)?;
            let page_start = value_page * self.root.page_size;
            let slice_start = start.saturating_sub(page_start) as usize;
            let slice_end = (end.min(page_start + self.root.page_size) - page_start) as usize;
            if slice_end > page.len() || slice_start > slice_end {
                return None;
            }
            out.extend_from_slice(&page[slice_start..slice_end]);
        }
        Some(out)
    }

    fn record_missing_external_value_pages(
        &self,
        start: u32,
        len: u32,
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Result<(), String> {
        if len == 0 {
            return Ok(());
        }
        let end = start
            .checked_add(len)
            .ok_or_else(|| "nav_kv external value range overflows".to_string())?;
        if end > self.root.value_bytes_len {
            return Err("nav_kv external value extends past value bytes".to_string());
        }
        let start_page = start / self.root.page_size;
        let end_page = (end - 1) / self.root.page_size;
        for value_page in start_page..=end_page {
            let page_index = self.root.value_page_start + value_page;
            self.value_page_present_or_record_missing(page_index, missing_pages, stats);
        }
        Ok(())
    }

    fn node_page_bytes_or_record_missing<'a>(
        &'a self,
        page: u32,
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Option<&'a [u8]> {
        if page >= self.root.value_page_start {
            return None;
        }
        match self.pages.get(&page) {
            Some(bytes) => {
                stats.node_page_hits += 1;
                Some(bytes.as_slice())
            }
            None => {
                stats.node_page_misses += 1;
                missing_pages.insert(page);
                None
            }
        }
    }

    fn value_page_bytes_or_record_missing<'a>(
        &'a self,
        page: u32,
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> Option<&'a [u8]> {
        match self.pages.get(&page) {
            Some(bytes) => {
                stats.value_page_hits += 1;
                Some(bytes.as_slice())
            }
            None => {
                stats.value_page_misses += 1;
                missing_pages.insert(page);
                None
            }
        }
    }

    fn value_page_present_or_record_missing(
        &self,
        page: u32,
        missing_pages: &mut BTreeSet<u32>,
        stats: &mut NavKvPageProbeStats,
    ) -> bool {
        match self.pages.contains_key(&page) {
            true => {
                stats.value_page_hits += 1;
                true
            }
            false => {
                stats.value_page_misses += 1;
                missing_pages.insert(page);
                false
            }
        }
    }

    fn page_bytes_or_record_missing(
        &self,
        page: u32,
        missing_pages: &mut BTreeSet<u32>,
    ) -> Option<Vec<u8>> {
        match self.pages.get(&page) {
            Some(bytes) => Some(bytes.clone()),
            None => {
                missing_pages.insert(page);
                None
            }
        }
    }
}

fn internal_child_for_key_ref(bytes: &[u8], key: &[u8]) -> Result<u32, String> {
    let pivot_count = read_u32(bytes, 4)? as usize;
    let child_count = read_u32(bytes, 8)? as usize;
    if child_count != pivot_count + 1 {
        return Err("nav_kv internal child/pivot count mismatch".to_string());
    }
    let children_offset = INTERNAL_HEADER_LEN;
    let pivots_offset = children_offset
        .checked_add(
            child_count
                .checked_mul(4)
                .ok_or_else(|| "nav_kv internal child table overflows".to_string())?,
        )
        .ok_or_else(|| "nav_kv internal child table overflows".to_string())?;
    let mut child_index = 0usize;
    let mut offset = pivots_offset;
    for pivot_index in 0..pivot_count {
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let key_end = offset
            .checked_add(key_len)
            .ok_or_else(|| "nav_kv internal pivot length overflows".to_string())?;
        let pivot = bytes
            .get(offset..key_end)
            .ok_or_else(|| "nav_kv internal pivot extends past page".to_string())?;
        if pivot <= key {
            child_index = pivot_index + 1;
        } else {
            break;
        }
        offset = key_end;
    }
    read_u32(bytes, children_offset + child_index * 4)
}

fn leaf_entry_value_ref_for_key<'a>(
    bytes: &'a [u8],
    key: &[u8],
    stats: &mut NavKvPageProbeStats,
) -> Result<Option<LeafEntryValueRef<'a>>, String> {
    let count = read_u32(bytes, 4)? as usize;
    let mut offset = LEAF_HEADER_LEN;
    for _ in 0..count {
        stats.leaf_entries_scanned += 1;
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let value_kind = read_u32(bytes, offset)?;
        offset += 4;
        let value_a = read_u32(bytes, offset)?;
        offset += 4;
        let value_b = read_u32(bytes, offset)?;
        offset += 4;

        let key_end = offset
            .checked_add(key_len)
            .ok_or_else(|| "nav_kv leaf key length overflows".to_string())?;
        let entry_key = bytes
            .get(offset..key_end)
            .ok_or_else(|| "nav_kv leaf key extends past page".to_string())?;
        offset = key_end;

        match entry_key.cmp(key) {
            Ordering::Equal => match value_kind {
                VALUE_KIND_INLINE => {
                    stats.inline_values += 1;
                    let value_len = value_a as usize;
                    let value_end = offset
                        .checked_add(value_len)
                        .ok_or_else(|| "nav_kv inline value length overflows".to_string())?;
                    let value = bytes
                        .get(offset..value_end)
                        .ok_or_else(|| "nav_kv inline value extends past page".to_string())?;
                    return Ok(Some(LeafEntryValueRef::Inline(value)));
                }
                VALUE_KIND_EXTERNAL => {
                    stats.external_values += 1;
                    return Ok(Some(LeafEntryValueRef::External {
                        offset: value_a,
                        len: value_b,
                    }));
                }
                _ => return Err("nav_kv leaf entry has invalid value kind".to_string()),
            },
            Ordering::Greater => return Ok(None),
            Ordering::Less => match value_kind {
                VALUE_KIND_INLINE => {
                    let value_len = value_a as usize;
                    offset = offset
                        .checked_add(value_len)
                        .ok_or_else(|| "nav_kv inline value length overflows".to_string())?;
                    if offset > bytes.len() {
                        return Err("nav_kv inline value extends past page".to_string());
                    }
                }
                VALUE_KIND_EXTERNAL => {}
                _ => return Err("nav_kv leaf entry has invalid value kind".to_string()),
            },
        }
    }
    Ok(None)
}

fn validate_pairs(pairs: &[NavKvPair], page_size: u32) -> Result<(), String> {
    if page_size == 0 {
        return Err("nav_kv page_size must be non-zero".to_string());
    }
    if pairs.is_empty() {
        return Err("nav_kv requires at least one key/value pair".to_string());
    }
    let page_size_usize = usize::try_from(page_size)
        .map_err(|_| "nav_kv page size does not fit usize".to_string())?;
    let mut previous_key: Option<&str> = None;
    for pair in pairs {
        if pair.key.is_empty() {
            return Err("nav_kv key must not be empty".to_string());
        }
        if pair.value.is_empty() {
            return Err(format!(
                "nav_kv value for key {} must not be empty",
                pair.key
            ));
        }
        let inline_value_len = if pair.value.len() <= INLINE_VALUE_MAX_LEN {
            pair.value.len()
        } else {
            0
        };
        if LEAF_HEADER_LEN + LEAF_ENTRY_FIXED_LEN + pair.key.len() + inline_value_len
            > page_size_usize
        {
            return Err(format!(
                "nav_kv key {} is too large for a leaf page",
                pair.key
            ));
        }
        if let Some(previous) = previous_key {
            match previous.as_bytes().cmp(pair.key.as_bytes()) {
                Ordering::Less => {}
                Ordering::Equal => return Err(format!("duplicate nav_kv key {}", pair.key)),
                Ordering::Greater => {
                    return Err(format!(
                        "nav_kv keys are not sorted: {previous} before {}",
                        pair.key
                    ))
                }
            }
        }
        previous_key = Some(&pair.key);
    }
    Ok(())
}

fn validate_sorted_unique_pairs(pairs: &[NavKvPair]) -> Result<(), String> {
    let mut previous_key: Option<&str> = None;
    for pair in pairs {
        if pair.key.is_empty() {
            return Err("nav_kv key must not be empty".to_string());
        }
        if pair.value.is_empty() {
            return Err(format!(
                "nav_kv value for key {} must not be empty",
                pair.key
            ));
        }
        if let Some(previous) = previous_key {
            match previous.cmp(pair.key.as_str()) {
                Ordering::Less => {}
                Ordering::Equal => return Err(format!("nav_kv duplicate key {}", pair.key)),
                Ordering::Greater => {
                    return Err(format!(
                        "nav_kv keys are not sorted: {previous} before {}",
                        pair.key
                    ));
                }
            }
        }
        previous_key = Some(&pair.key);
    }
    Ok(())
}

fn validate_sorted_unique_delta(delta: &NavKvDelta) -> Result<(), String> {
    let mut previous_key: Option<&str> = None;
    for entry in &delta.entries {
        if entry.key.is_empty() {
            return Err("nav_kv delta key must not be empty".to_string());
        }
        if entry.value.as_ref().is_some_and(Vec::is_empty) {
            return Err(format!(
                "nav_kv delta value for key {} must not be empty",
                entry.key
            ));
        }
        if let Some(previous) = previous_key {
            match previous.cmp(entry.key.as_str()) {
                Ordering::Less => {}
                Ordering::Equal => return Err(format!("nav_kv delta duplicate key {}", entry.key)),
                Ordering::Greater => {
                    return Err(format!(
                        "nav_kv delta keys are not sorted: {previous} before {}",
                        entry.key
                    ));
                }
            }
        }
        previous_key = Some(&entry.key);
    }
    Ok(())
}

fn build_leaf_pages(
    entries: &[LeafEntry],
    page_size: usize,
    pages: &mut Vec<Vec<u8>>,
) -> Result<Vec<NodeSummary>, String> {
    let mut groups: Vec<Vec<LeafEntry>> = Vec::new();
    let mut current = Vec::new();
    let mut current_size = LEAF_HEADER_LEN;
    for entry in entries {
        let entry_size = leaf_entry_encoded_len(entry);
        if !current.is_empty() && current_size + entry_size > page_size {
            groups.push(current);
            current = Vec::new();
            current_size = LEAF_HEADER_LEN;
        }
        if current_size + entry_size > page_size {
            return Err("nav_kv leaf entry exceeds page size".to_string());
        }
        current_size += entry_size;
        current.push(entry.clone());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    let first_leaf_page =
        u32::try_from(pages.len()).map_err(|_| "nav_kv page count exceeds u32".to_string())?;
    let mut summaries = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        let page = first_leaf_page + index as u32;
        let next_leaf = if index + 1 < groups.len() {
            Some(page + 1)
        } else {
            None
        };
        pages.push(encode_leaf_page(group, next_leaf)?);
        summaries.push(NodeSummary {
            page,
            first_key: group[0].key.clone(),
        });
    }
    Ok(summaries)
}

fn leaf_entry_encoded_len(entry: &LeafEntry) -> usize {
    LEAF_ENTRY_FIXED_LEN
        + entry.key.len()
        + match &entry.value {
            LeafEntryValue::Inline(bytes) => bytes.len(),
            LeafEntryValue::External { .. } => 0,
        }
}

fn build_internal_level(
    children: &[NodeSummary],
    page_size: usize,
    pages: &mut Vec<Vec<u8>>,
) -> Result<Vec<NodeSummary>, String> {
    let groups = pack_internal_groups(children, page_size)?;
    let mut summaries = Vec::with_capacity(groups.len());
    for group in groups {
        let page =
            u32::try_from(pages.len()).map_err(|_| "nav_kv page count exceeds u32".to_string())?;
        pages.push(encode_internal_page(&group)?);
        summaries.push(NodeSummary {
            page,
            first_key: group[0].first_key.clone(),
        });
    }
    Ok(summaries)
}

fn pack_internal_groups(
    children: &[NodeSummary],
    page_size: usize,
) -> Result<Vec<Vec<NodeSummary>>, String> {
    let mut groups: Vec<Vec<NodeSummary>> = Vec::new();
    let mut current = Vec::new();
    for child in children {
        let candidate_len = current.len() + 1;
        let candidate_size = internal_page_size_for_children(&current, Some(child));
        if candidate_len > 1 && candidate_size > page_size {
            groups.push(current);
            current = Vec::new();
        }
        current.push(child.clone());
        if internal_page_size_for_children(&current, None) > page_size {
            return Err("nav_kv internal child entry exceeds page size".to_string());
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    if groups.len() > 1 && groups.last().is_some_and(|group| group.len() == 1) {
        let last = groups.pop().unwrap().pop().unwrap();
        let previous = groups
            .last_mut()
            .ok_or_else(|| "nav_kv internal grouping underflow".to_string())?;
        if previous.len() <= 2 {
            previous.push(last);
            if internal_page_size_for_children(previous, None) > page_size {
                return Err("nav_kv internal page exceeds page size after regrouping".to_string());
            }
        } else {
            let moved = previous.pop().unwrap();
            groups.push(vec![moved, last]);
        }
    }
    Ok(groups)
}

fn internal_page_size_for_children(children: &[NodeSummary], extra: Option<&NodeSummary>) -> usize {
    let child_count = children.len() + usize::from(extra.is_some());
    let mut size = INTERNAL_HEADER_LEN + child_count * 4;
    for child in children.iter().skip(1) {
        size += 4 + child.first_key.len();
    }
    if let Some(extra) = extra {
        if child_count > 1 {
            size += 4 + extra.first_key.len();
        }
    }
    size
}

fn encode_leaf_page(entries: &[LeafEntry], next_leaf: Option<u32>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    push_u32(&mut out, NODE_KIND_LEAF);
    push_u32(
        &mut out,
        u32::try_from(entries.len()).map_err(|_| "too many leaf entries".to_string())?,
    );
    push_u32(&mut out, next_leaf.unwrap_or(NO_PAGE));
    for entry in entries {
        push_u32(
            &mut out,
            u32::try_from(entry.key.len())
                .map_err(|_| "nav_kv key length exceeds u32".to_string())?,
        );
        match &entry.value {
            LeafEntryValue::Inline(bytes) => {
                push_u32(&mut out, VALUE_KIND_INLINE);
                push_u32(
                    &mut out,
                    u32::try_from(bytes.len())
                        .map_err(|_| "nav_kv inline value length exceeds u32".to_string())?,
                );
                push_u32(&mut out, 0);
            }
            LeafEntryValue::External { offset, len } => {
                push_u32(&mut out, VALUE_KIND_EXTERNAL);
                push_u32(&mut out, *offset);
                push_u32(&mut out, *len);
            }
        }
        out.extend_from_slice(&entry.key);
        if let LeafEntryValue::Inline(bytes) = &entry.value {
            out.extend_from_slice(bytes);
        }
    }
    Ok(out)
}

fn encode_internal_page(children: &[NodeSummary]) -> Result<Vec<u8>, String> {
    if children.len() < 2 {
        return Err("nav_kv internal nodes require at least two children".to_string());
    }
    let mut out = Vec::new();
    push_u32(&mut out, NODE_KIND_INTERNAL);
    push_u32(
        &mut out,
        u32::try_from(children.len() - 1).map_err(|_| "too many pivots".to_string())?,
    );
    push_u32(
        &mut out,
        u32::try_from(children.len()).map_err(|_| "too many children".to_string())?,
    );
    for child in children {
        push_u32(&mut out, child.page);
    }
    for child in children.iter().skip(1) {
        push_u32(
            &mut out,
            u32::try_from(child.first_key.len())
                .map_err(|_| "nav_kv pivot length exceeds u32".to_string())?,
        );
        out.extend_from_slice(&child.first_key);
    }
    Ok(out)
}

fn parse_leaf_node(bytes: &[u8]) -> Result<LeafNode, String> {
    let count = read_u32(bytes, 4)? as usize;
    let next_raw = read_u32(bytes, 8)?;
    let mut offset = LEAF_HEADER_LEN;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let value_kind = read_u32(bytes, offset)?;
        offset += 4;
        let value_a = read_u32(bytes, offset)?;
        offset += 4;
        let value_b = read_u32(bytes, offset)?;
        offset += 4;
        let key = bytes
            .get(offset..offset + key_len)
            .ok_or_else(|| "nav_kv leaf key extends past page".to_string())?
            .to_vec();
        offset += key_len;
        let value = match value_kind {
            VALUE_KIND_INLINE => {
                let value_len = value_a as usize;
                let value = bytes
                    .get(offset..offset + value_len)
                    .ok_or_else(|| "nav_kv inline value extends past page".to_string())?
                    .to_vec();
                offset += value_len;
                LeafEntryValue::Inline(value)
            }
            VALUE_KIND_EXTERNAL => LeafEntryValue::External {
                offset: value_a,
                len: value_b,
            },
            _ => return Err("nav_kv leaf entry has invalid value kind".to_string()),
        };
        entries.push(LeafLookup { key, value });
    }
    Ok(LeafNode {
        next_leaf: (next_raw != NO_PAGE).then_some(next_raw),
        entries,
    })
}

fn parse_internal_node(bytes: &[u8]) -> Result<InternalNode, String> {
    let pivot_count = read_u32(bytes, 4)? as usize;
    let child_count = read_u32(bytes, 8)? as usize;
    if child_count != pivot_count + 1 {
        return Err("nav_kv internal child/pivot count mismatch".to_string());
    }
    let mut offset = INTERNAL_HEADER_LEN;
    let mut children = Vec::with_capacity(child_count);
    for _ in 0..child_count {
        children.push(read_u32(bytes, offset)?);
        offset += 4;
    }
    let mut pivots = Vec::with_capacity(pivot_count);
    for _ in 0..pivot_count {
        let key_len = read_u32(bytes, offset)? as usize;
        offset += 4;
        let key = bytes
            .get(offset..offset + key_len)
            .ok_or_else(|| "nav_kv internal pivot extends past page".to_string())?
            .to_vec();
        offset += key_len;
        pivots.push(key);
    }
    Ok(InternalNode { children, pivots })
}

fn startup_prefetch_pages(root: &NavKvRoot, pages: &[Vec<u8>]) -> Result<Vec<u32>, String> {
    let mut touched = BTreeSet::new();
    trace_extract_value(root, pages, &mut touched, "contract/nav-db")?;
    if !trace_extract_value(root, pages, &mut touched, "chart/catalog")? {
        return Ok(Vec::new());
    }
    trace_extract_value(
        root,
        pages,
        &mut touched,
        "weather/metar-important-stations",
    )?;
    let package_keys = trace_prefix_keys(root, pages, &mut touched, "package/by-id/")?;
    for key in package_keys {
        trace_extract_value(root, pages, &mut touched, &key)?;
    }
    trace_extract_value(root, pages, &mut touched, "vector/manifest")?;
    Ok(touched.into_iter().collect())
}

fn trace_extract_value(
    root: &NavKvRoot,
    pages: &[Vec<u8>],
    touched: &mut BTreeSet<u32>,
    key: &str,
) -> Result<bool, String> {
    Ok(root
        .extract_value(key, |page| trace_page(pages, touched, page))
        .is_some())
}

fn trace_prefix_keys(
    root: &NavKvRoot,
    pages: &[Vec<u8>],
    touched: &mut BTreeSet<u32>,
    prefix: &str,
) -> Result<Vec<String>, String> {
    root.prefix_keys(prefix, |page| trace_page(pages, touched, page))
        .ok_or_else(|| format!("nav_kv startup prefetch failed to scan prefix {prefix}"))
}

fn trace_page(pages: &[Vec<u8>], touched: &mut BTreeSet<u32>, page: u32) -> Option<Vec<u8>> {
    touched.insert(page);
    pages.get(page as usize).cloned()
}

fn build_root_bytes(
    entry_count: u32,
    page_size: u32,
    root_page: u32,
    page_count: u32,
    value_page_start: u32,
    value_bytes_len: u32,
    prefetch_pages: &[u32],
) -> Result<Vec<u8>, String> {
    let prefetch_count = u32::try_from(prefetch_pages.len())
        .map_err(|_| "nav_kv prefetch page count exceeds u32".to_string())?;
    let mut root_bytes = Vec::with_capacity(HEADER_LEN + prefetch_pages.len() * 4);
    root_bytes.extend_from_slice(MAGIC);
    push_u32(&mut root_bytes, NAVKV_STORAGE_FORMAT);
    push_u32(&mut root_bytes, entry_count);
    push_u32(&mut root_bytes, page_size);
    push_u32(&mut root_bytes, root_page);
    push_u32(&mut root_bytes, page_count);
    push_u32(&mut root_bytes, value_page_start);
    push_u32(&mut root_bytes, value_bytes_len);
    while root_bytes.len() < PREFETCH_COUNT_OFFSET {
        push_u32(&mut root_bytes, 0);
    }
    push_u32(&mut root_bytes, prefetch_count);
    while root_bytes.len() < HEADER_LEN {
        push_u32(&mut root_bytes, 0);
    }
    for page in prefetch_pages {
        push_u32(&mut root_bytes, *page);
    }
    Ok(root_bytes)
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let chunk = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "nav_kv read past end".to_string())?;
    Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const TEST_PAGE_SIZE: u32 = 256;

    fn pair(key: &str, value: &str) -> NavKvPair {
        NavKvPair {
            key: key.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }

    #[test]
    fn exact_lookup_extracts_value() {
        let built = build_nav_kv_sorted(
            vec![
                pair("waypoint/id/KRDD", "{\"id\":\"KRDD\"}"),
                pair("chart/catalog", "{}"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let value = root
            .extract_value("waypoint/id/KRDD", |page| {
                built.pages.get(page as usize).cloned()
            })
            .expect("value");
        assert_eq!(value, b"{\"id\":\"KRDD\"}");
    }

    #[test]
    fn missing_lookup_returns_none() {
        let built = build_nav_kv_sorted(vec![pair("chart/catalog", "{}")], TEST_PAGE_SIZE)
            .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        assert_eq!(
            root.get_value_range("missing", |page| built.pages.get(page as usize).cloned()),
            None
        );
        assert_eq!(
            root.extract_value("missing", |page| built.pages.get(page as usize).cloned()),
            None
        );
    }

    #[test]
    fn prefix_lookup_walks_leaf_links() {
        let built = build_nav_kv_sorted(
            vec![
                pair("waypoint/id/KRDD", "1"),
                pair("waypoint/id/KRNT", "2"),
                pair("waypoint/suggest/KR", "3"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        assert_eq!(
            root.prefix_keys("waypoint/id/", |page| built
                .pages
                .get(page as usize)
                .cloned())
                .expect("prefix keys"),
            vec![
                "waypoint/id/KRDD".to_string(),
                "waypoint/id/KRNT".to_string()
            ]
        );
    }

    #[test]
    fn prefix_stats_reports_payload_sizes_and_storage_pages() {
        let external_value = "x".repeat(INLINE_VALUE_MAX_LEN + 200);
        let built = build_nav_kv_sorted(
            vec![
                pair("magvar/0/0", "1234"),
                NavKvPair {
                    key: "magvar/0/1".to_string(),
                    value: external_value.as_bytes().to_vec(),
                },
                pair("waypoint/id/KRDD", "unrelated"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let stats = root
            .prefix_stats("magvar/", |page| built.pages.get(page as usize).cloned())
            .expect("prefix stats");

        assert_eq!(stats.key_count, 2);
        assert_eq!(stats.key_bytes, "magvar/0/0".len() + "magvar/0/1".len());
        assert_eq!(stats.value_bytes, 4 + INLINE_VALUE_MAX_LEN + 200);
        assert_eq!(stats.inline_value_count, 1);
        assert_eq!(stats.external_value_count, 1);
        assert!(!stats.matching_leaf_pages.is_empty());
        assert!(!stats.external_value_pages.is_empty());
    }

    #[test]
    fn value_can_cross_page_boundaries() {
        let value = "x".repeat(INLINE_VALUE_MAX_LEN + 200);
        let built = build_nav_kv_sorted(
            vec![NavKvPair {
                key: "k".to_string(),
                value: value.as_bytes().to_vec(),
            }],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        assert!(built.pages.len() > 1);
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let value = root
            .extract_value("k", |page| built.pages.get(page as usize).cloned())
            .expect("value");
        assert_eq!(value, "x".repeat(INLINE_VALUE_MAX_LEN + 200).as_bytes());
    }

    #[test]
    fn store_probe_collects_missing_pages_without_materializing_values() {
        let external_value = "x".repeat(INLINE_VALUE_MAX_LEN + 200);
        let built = build_nav_kv_sorted(
            vec![
                pair("a", "inline"),
                NavKvPair {
                    key: "b".to_string(),
                    value: external_value.into_bytes(),
                },
                pair("c", "inline"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let value_page_start = root.value_page_start;
        let mut store = NavKvStore::new(root);
        for page in 0..value_page_start {
            store.insert_page(page, built.pages[page as usize].clone());
        }

        let keys = vec!["a".to_string(), "b".to_string(), "missing".to_string()];
        let (missing_pages, stats) = store
            .missing_pages_for_keys_with_stats(&keys)
            .expect("probe pages");

        assert!(!missing_pages.is_empty());
        assert_eq!(stats.keys, 3);
        assert!(stats.inline_values >= 1);
        assert!(stats.external_values >= 1);
        assert_eq!(stats.value_page_hits, 0);
        assert_eq!(stats.value_page_misses, missing_pages.len());
        assert!(matches!(
            store.get_bytes("b").expect("lookup b"),
            NavKvLookup::MissingPages(_)
        ));
    }

    #[test]
    fn store_probe_reports_no_missing_pages_when_external_value_pages_are_loaded() {
        let external_value = "x".repeat(INLINE_VALUE_MAX_LEN + 200);
        let built = build_nav_kv_sorted(
            vec![NavKvPair {
                key: "b".to_string(),
                value: external_value.as_bytes().to_vec(),
            }],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let mut store = NavKvStore::new(root);
        for (index, page) in built.pages.iter().enumerate() {
            store.insert_page(index as u32, page.clone());
        }

        let keys = vec!["b".to_string()];
        let (missing_pages, stats) = store
            .missing_pages_for_keys_with_stats(&keys)
            .expect("probe pages");

        assert!(missing_pages.is_empty());
        assert_eq!(stats.keys, 1);
        assert_eq!(stats.external_values, 1);
        assert!(stats.value_page_hits > 0);
        assert_eq!(
            store.get_bytes("b").expect("lookup b"),
            NavKvLookup::Hit(external_value.into_bytes())
        );
    }

    #[test]
    fn strict_builder_rejects_unsorted_keys() {
        let err = build_nav_kv_strict(vec![pair("b", "1"), pair("a", "2")], TEST_PAGE_SIZE)
            .expect_err("unsorted keys should fail");
        assert!(err.contains("not sorted"));
    }

    #[test]
    fn builder_rejects_duplicate_keys() {
        let err = build_nav_kv_sorted(vec![pair("a", "1"), pair("a", "2")], TEST_PAGE_SIZE)
            .expect_err("duplicate keys should fail");
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn builder_rejects_zero_length_values() {
        let err = build_nav_kv_sorted(
            vec![NavKvPair {
                key: "a".to_string(),
                value: Vec::new(),
            }],
            TEST_PAGE_SIZE,
        )
        .expect_err("empty value should fail");
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn parser_rejects_bad_magic() {
        let built =
            build_nav_kv_sorted(vec![pair("a", "1")], TEST_PAGE_SIZE).expect("build nav kv");
        let mut root = built.root_bytes;
        root[0] = b'X';
        let err = NavKvRoot::parse(&root).expect_err("bad magic should fail");
        assert!(err.contains("invalid magic"));
    }

    #[test]
    fn repeated_extraction_can_reuse_cached_pages() {
        let built =
            build_nav_kv_sorted(vec![pair("a", "abcde"), pair("b", "fghij")], TEST_PAGE_SIZE)
                .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        let mut cache = HashMap::<u32, Vec<u8>>::new();
        let mut misses = 0;
        fn extract_with_cache(
            root: &NavKvRoot,
            built: &NavKvBuildOutput,
            cache: &mut HashMap<u32, Vec<u8>>,
            misses: &mut u32,
        ) -> Vec<u8> {
            root.extract_value("a", |page| {
                if let Some(cached) = cache.get(&page) {
                    return Some(cached.clone());
                }
                *misses += 1;
                let loaded = built.pages.get(page as usize)?.clone();
                cache.insert(page, loaded.clone());
                Some(loaded)
            })
            .expect("value")
        }
        assert_eq!(
            extract_with_cache(&root, &built, &mut cache, &mut misses),
            b"abcde"
        );
        let first_lookup_misses = misses;
        assert!(first_lookup_misses > 0);
        assert_eq!(
            extract_with_cache(&root, &built, &mut cache, &mut misses),
            b"abcde"
        );
        assert_eq!(misses, first_lookup_misses);
    }

    #[test]
    fn root_prefetch_pages_cover_startup_keys_and_package_values() {
        let built = build_nav_kv_sorted(
            vec![
                pair("chart/catalog", "catalog-value"),
                pair("package/by-id/a", "package-a"),
                pair("package/by-id/b", "package-b"),
                pair("weather/metar-important-stations", "metar-importance"),
                pair("vector/manifest", "vector-manifest"),
                pair("waypoint/id/KRDD", "unrelated"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        assert_eq!(root.prefetch_pages(), built.prefetch_pages);
        assert!(!root.prefetch_pages().is_empty());

        let mut cache = HashMap::<u32, Vec<u8>>::new();
        for page in root.prefetch_pages() {
            cache.insert(*page, built.pages[*page as usize].clone());
        }
        assert_eq!(
            root.extract_value("chart/catalog", |page| cache.get(&page).cloned())
                .as_deref(),
            Some(b"catalog-value".as_slice())
        );
        assert_eq!(
            root.prefix_keys("package/by-id/", |page| cache.get(&page).cloned())
                .expect("package keys"),
            vec!["package/by-id/a".to_string(), "package/by-id/b".to_string()]
        );
        assert_eq!(
            root.extract_value("package/by-id/a", |page| cache.get(&page).cloned())
                .as_deref(),
            Some(b"package-a".as_slice())
        );
        assert_eq!(
            root.extract_value("package/by-id/b", |page| cache.get(&page).cloned())
                .as_deref(),
            Some(b"package-b".as_slice())
        );
        assert_eq!(
            root.extract_value("vector/manifest", |page| cache.get(&page).cloned())
                .as_deref(),
            Some(b"vector-manifest".as_slice())
        );
        assert_eq!(
            root.extract_value("weather/metar-important-stations", |page| cache
                .get(&page)
                .cloned())
                .as_deref(),
            Some(b"metar-importance".as_slice())
        );
    }

    #[test]
    fn missing_prefetch_pages_excludes_pages_already_loaded() {
        let built = build_nav_kv_sorted(
            vec![
                pair("chart/catalog", "catalog-value"),
                pair("package/by-id/a", "package-a"),
                pair("package/by-id/b", "package-b"),
                pair("weather/metar-important-stations", "metar-importance"),
                pair("vector/manifest", "vector-manifest"),
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
        assert_eq!(root.prefetch_pages(), built.prefetch_pages);
        assert!(!root.prefetch_pages().is_empty());

        let mut store = NavKvStore::new(root.clone());
        assert_eq!(store.missing_prefetch_pages(), root.prefetch_pages());

        let first_page = root.prefetch_pages()[0];
        store.insert_page(first_page, built.pages[first_page as usize].clone());
        assert_eq!(
            store.missing_prefetch_pages(),
            root.prefetch_pages()[1..].to_vec()
        );

        for page in root.prefetch_pages()[1..].iter().copied() {
            store.insert_page(page, built.pages[page as usize].clone());
        }
        assert!(store.missing_prefetch_pages().is_empty());
    }

    #[test]
    fn full_pair_iteration_returns_canonical_key_order() {
        let built = build_nav_kv_sorted(
            vec![
                pair("z", "last"),
                pair("a", "first"),
                NavKvPair {
                    key: "m".to_string(),
                    value: "x".repeat(INLINE_VALUE_MAX_LEN + 10).into_bytes(),
                },
            ],
            TEST_PAGE_SIZE,
        )
        .expect("build nav kv");
        let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");

        let pairs = root
            .pairs(|page| built.pages.get(page as usize).cloned())
            .expect("pairs");

        assert_eq!(
            pairs
                .iter()
                .map(|pair| pair.key.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "m", "z"]
        );
        assert_eq!(
            pairs[1].value,
            "x".repeat(INLINE_VALUE_MAX_LEN + 10).into_bytes()
        );
        assert_eq!(
            root.canonical_sha256(|page| built.pages.get(page as usize).cloned()),
            Some(nav_kv_canonical_sha256_from_pairs(&pairs))
        );
    }

    #[test]
    fn had_delta_replaces_adds_and_deletes_keys_to_converge_hashes() {
        let from = vec![pair("a", "old"), pair("b", "delete-me"), pair("d", "same")];
        let to = vec![pair("a", "new"), pair("c", "added"), pair("d", "same")];

        let delta = build_nav_kv_delta(&from, &to).expect("delta");

        assert_eq!(
            delta.entries,
            vec![
                NavKvDeltaEntry {
                    key: "a".to_string(),
                    value: Some(b"new".to_vec()),
                },
                NavKvDeltaEntry {
                    key: "b".to_string(),
                    value: None,
                },
                NavKvDeltaEntry {
                    key: "c".to_string(),
                    value: Some(b"added".to_vec()),
                },
            ]
        );
        let applied = apply_nav_kv_delta(&from, &delta).expect("apply delta");
        assert_eq!(applied, to);
        assert_eq!(
            nav_kv_canonical_sha256_from_pairs(&applied),
            nav_kv_canonical_sha256_from_pairs(&to)
        );
        assert!(
            applied.iter().all(|pair| pair.key != "b"),
            "deleted key was still present after applying delta"
        );
    }
}

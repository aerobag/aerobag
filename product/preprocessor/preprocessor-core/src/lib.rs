use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PACKAGE_ASSET_MANIFEST_NAME: &str = "package-assets.json";

pub mod nav_kv {
    use std::cmp::Ordering;
    use std::collections::BTreeSet;

    pub const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
    pub const VERSION: u32 = 4;
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

    impl NavKvRoot {
        pub fn parse(root_bytes: &[u8]) -> Result<Self, String> {
            if root_bytes.len() < HEADER_LEN {
                return Err("nav_kv root is shorter than header".to_string());
            }
            if &root_bytes[0..16] != MAGIC {
                return Err("nav_kv root has invalid magic".to_string());
            }
            let version = read_u32(root_bytes, 16)?;
            if version != VERSION {
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
            let mut leaf_page =
                self.find_leaf_page_for_key(prefix.as_bytes(), &mut page_provider)?;
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
                        let child_index =
                            node.pivots.partition_point(|pivot| pivot.as_slice() <= key);
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
            let page = u32::try_from(pages.len())
                .map_err(|_| "nav_kv page count exceeds u32".to_string())?;
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
                    return Err(
                        "nav_kv internal page exceeds page size after regrouping".to_string()
                    );
                }
            } else {
                let moved = previous.pop().unwrap();
                groups.push(vec![moved, last]);
            }
        }
        Ok(groups)
    }

    fn internal_page_size_for_children(
        children: &[NodeSummary],
        extra: Option<&NodeSummary>,
    ) -> usize {
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
        if !trace_extract_value(root, pages, &mut touched, "chart/catalog")? {
            return Ok(Vec::new());
        }
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
        push_u32(&mut root_bytes, VERSION);
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageAssetManifest {
    pub schema_version: u32,
    pub family_id: String,
    pub package_id: String,
    pub assets: Vec<PackageAssetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PackageAssetRecord {
    pub id: String,
    pub airport_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icao_airport_id: Option<String>,
    pub label: String,
    pub asset_kind: String,
    pub document_type: String,
    pub asset_path: String,
    pub thumbnail_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub procedure_uid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub georef: Option<PlateGeoref>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlateGeoref {
    PlateTransformV1 {
        pixels_per_longitude: f64,
        pixels_per_latitude: f64,
        top_left_lon: f64,
        top_left_lat: f64,
    },
    AirportDiagramTransformV1 {
        pixel_x_from_lon: f64,
        pixel_x_from_lat: f64,
        pixel_x_offset: f64,
        pixel_y_from_lon: f64,
        pixel_y_from_lat: f64,
        pixel_y_offset: f64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureManifest {
    pub schema_version: u32,
    pub run_id: String,
    pub created_at_utc: String,
    pub captures: Vec<CaptureEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEntry {
    pub label: String,
    pub repo: String,
    pub command: Vec<String>,
    pub stdout_log: String,
    pub stderr_log: String,
    #[serde(default)]
    pub tile_paths: Option<String>,
    pub outputs_hashes: String,
    #[serde(default)]
    pub source_urls: Option<String>,
    #[serde(default)]
    pub downloads: Option<String>,
    #[serde(default)]
    pub package_outputs: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedTileCounts {
    pub sec: u64,
    pub tac: u64,
    pub enr_l: u64,
}

impl ExpectedTileCounts {
    pub const CURRENT_BASELINE: Self = Self {
        sec: 35_494,
        tac: 7_174,
        enr_l: 27_428,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartFamily {
    Sec,
    Tac,
    EnrL,
    EnrH,
}

impl ChartFamily {
    pub fn capture_label(self) -> &'static str {
        match self {
            Self::Sec => "charts-sec",
            Self::Tac => "charts-tac",
            Self::EnrL => "charts-enr-l",
            Self::EnrH => "charts-enr-h",
        }
    }

    pub fn baseline_tile_count(self) -> Option<u64> {
        match self {
            Self::Sec => Some(ExpectedTileCounts::CURRENT_BASELINE.sec),
            Self::Tac => Some(ExpectedTileCounts::CURRENT_BASELINE.tac),
            Self::EnrL => Some(ExpectedTileCounts::CURRENT_BASELINE.enr_l),
            Self::EnrH => None,
        }
    }

    pub fn from_capture_label(label: &str) -> Option<Self> {
        match label {
            "charts-sec" => Some(Self::Sec),
            "charts-tac" => Some(Self::Tac),
            "charts-enr-l" => Some(Self::EnrL),
            "charts-enr-h" => Some(Self::EnrH),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionBounds {
    pub lon_min: f64,
    pub lat_max: f64,
    pub lon_max: f64,
    pub lat_min: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Ak,
    Pac,
    Nw,
    Sw,
    Nc,
    Ec,
    Sc,
    Ne,
    Se,
}

impl Region {
    pub const ALL: [Self; 9] = [
        Self::Ak,
        Self::Pac,
        Self::Nw,
        Self::Sw,
        Self::Nc,
        Self::Ec,
        Self::Sc,
        Self::Ne,
        Self::Se,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::Ak => "AK",
            Self::Pac => "PAC",
            Self::Nw => "NW",
            Self::Sw => "SW",
            Self::Nc => "NC",
            Self::Ec => "EC",
            Self::Sc => "SC",
            Self::Ne => "NE",
            Self::Se => "SE",
        }
    }

    pub fn from_code(value: &str) -> Option<Self> {
        match value {
            "AK" => Some(Self::Ak),
            "PAC" => Some(Self::Pac),
            "NW" => Some(Self::Nw),
            "SW" => Some(Self::Sw),
            "NC" => Some(Self::Nc),
            "EC" => Some(Self::Ec),
            "SC" => Some(Self::Sc),
            "NE" => Some(Self::Ne),
            "SE" => Some(Self::Se),
            _ => None,
        }
    }

    pub fn state_codes(self) -> &'static [&'static str] {
        match self {
            Self::Ak => &["AK"],
            Self::Pac => &["HI", "XX"],
            Self::Nw => &["WA", "MT", "WY", "ID", "OR"],
            Self::Sw => &["CA", "NV", "UT", "CO", "NM", "AZ"],
            Self::Nc => &["ND", "MN", "IA", "MO", "KS", "NE", "SD"],
            Self::Ec => &["WI", "MI", "OH", "IN", "IL"],
            Self::Sc => &["OK", "AR", "MS", "LA", "TX"],
            Self::Ne => &[
                "NY", "ME", "VT", "NH", "MA", "RI", "CT", "NJ", "DE", "MD", "DC", "VA", "WV", "PA",
            ],
            Self::Se => &["KY", "NC", "SC", "GA", "FL", "AL", "TN", "PR", "VI"],
        }
    }

    pub fn bounds(self) -> RegionBounds {
        self.bounds_list()[0]
    }

    pub fn bounds_list(self) -> &'static [RegionBounds] {
        match self {
            Self::Ak => {
                // The Alaska chart region crosses the antimeridian. Keep it as two
                // ordinary lon ranges so tile and app code never has to reason about a
                // single box that wraps around +/-180.
                &[
                    RegionBounds {
                        lon_min: -180.0,
                        lat_max: 72.0,
                        lon_max: -126.0,
                        lat_min: 51.0,
                    },
                    RegionBounds {
                        lon_min: 170.0,
                        lat_max: 56.0,
                        lon_max: 180.0,
                        lat_min: 51.0,
                    },
                ]
            }
            Self::Pac => {
                // PAC is not just Hawaii. The FAA Pacific SEC source includes
                // detailed island coverage for Hawaii, Samoa, and Guam/NMI; packaging
                // must admit all of those rectangles or the high-zoom source tiles get
                // thrown away before clients can probe them.
                &[
                    RegionBounds {
                        lon_min: -162.0,
                        lat_max: 24.0,
                        lon_max: -152.0,
                        lat_min: 18.0,
                    },
                    RegionBounds {
                        lon_min: -174.0,
                        lat_max: -11.0,
                        lon_max: -168.0,
                        lat_min: -16.0,
                    },
                    RegionBounds {
                        lon_min: 140.0,
                        lat_max: 22.0,
                        lon_max: 147.0,
                        lat_min: 10.0,
                    },
                ]
            }
            Self::Nw => &[RegionBounds {
                lon_min: -125.0,
                lat_max: 50.0,
                lon_max: -103.0,
                lat_min: 40.0,
            }],
            Self::Sw => &[RegionBounds {
                lon_min: -125.0,
                lat_max: 40.0,
                lon_max: -103.0,
                lat_min: 15.0,
            }],
            Self::Nc => &[RegionBounds {
                lon_min: -105.0,
                lat_max: 50.0,
                lon_max: -90.0,
                lat_min: 37.0,
            }],
            Self::Ec => &[RegionBounds {
                lon_min: -95.0,
                lat_max: 50.0,
                lon_max: -80.0,
                lat_min: 37.0,
            }],
            Self::Sc => &[RegionBounds {
                lon_min: -110.0,
                lat_max: 37.0,
                lon_max: -90.0,
                lat_min: 15.0,
            }],
            Self::Ne => &[RegionBounds {
                lon_min: -80.0,
                lat_max: 50.0,
                lon_max: -60.0,
                lat_min: 37.0,
            }],
            Self::Se => &[RegionBounds {
                lon_min: -90.0,
                lat_max: 37.0,
                lon_max: -60.0,
                lat_min: 14.0,
            }],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkKind {
    Network,
    Extract,
    Cpu,
    Io,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parallelism {
    Serial,
    Bounded,
    Wide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConcurrencyConfig {
    pub fetch_jobs: usize,
    pub extract_jobs: usize,
    pub cpu_jobs: usize,
    pub zip_jobs: usize,
}

impl ConcurrencyConfig {
    pub fn recommended_for_machine(cpus: usize) -> Self {
        let fetch_jobs = cpus.clamp(4, 12);
        let extract_jobs = (cpus / 2).clamp(2, 8);
        let cpu_jobs = cpus.saturating_sub(2).max(1);
        let zip_jobs = (cpus / 4).clamp(1, 4);
        Self {
            fetch_jobs,
            extract_jobs,
            cpu_jobs,
            zip_jobs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhasePlan {
    pub name: &'static str,
    pub work_kind: WorkKind,
    pub legacy_parallelism: Parallelism,
    pub rust_parallelism: Parallelism,
    pub recommended_jobs: usize,
    pub expected_bottleneck: &'static str,
    pub note: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    Online,
    Refresh,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPaths {
    pub root: PathBuf,
    pub logs: PathBuf,
    pub artifacts: PathBuf,
    pub meta: PathBuf,
}

impl RunPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            logs: root.join("logs"),
            artifacts: root.join("artifacts"),
            meta: root.join("meta"),
            root,
        }
    }
}

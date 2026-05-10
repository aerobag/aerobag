use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PACKAGE_ASSET_MANIFEST_NAME: &str = "package-assets.json";

pub mod nav_kv {
    use std::cmp::Ordering;
    use std::collections::HashSet;

    pub const MAGIC: &[u8; 16] = b"AEROBAGNAVKV0001";
    pub const VERSION: u32 = 2;
    pub const HEADER_LEN: usize = 64;
    pub const ENTRY_LEN: usize = 8;

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
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Entry {
        key_offset: u32,
        value_offset: u32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NavKvRoot {
        entry_count: u32,
        page_size: u32,
        offset_table_offset: u32,
        offset_table_len: u32,
        key_table_offset: u32,
        key_table_len: u32,
        value_table_offset: u32,
        value_bytes_len: u32,
        logical_bytes_len: u32,
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
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        if pairs.is_empty() {
            return Err("nav_kv requires at least one key/value pair".to_string());
        }

        let mut entries = Vec::with_capacity(pairs.len() + 1);
        let mut key_bytes = Vec::new();
        let mut value_bytes = Vec::new();
        let mut previous_key: Option<&str> = None;
        for pair in &pairs {
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
                match previous.as_bytes().cmp(pair.key.as_bytes()) {
                    Ordering::Less => {}
                    Ordering::Equal => {
                        return Err(format!("duplicate nav_kv key {}", pair.key));
                    }
                    Ordering::Greater => {
                        return Err(format!(
                            "nav_kv keys are not sorted: {previous} before {}",
                            pair.key
                        ));
                    }
                }
            }
            let key_offset = u32::try_from(key_bytes.len())
                .map_err(|_| "nav_kv key bytes exceed u32".to_string())?;
            let value_offset = u32::try_from(value_bytes.len())
                .map_err(|_| "nav_kv value bytes exceed u32".to_string())?;
            entries.push(Entry {
                key_offset,
                value_offset,
            });
            key_bytes.extend_from_slice(pair.key.as_bytes());
            value_bytes.extend_from_slice(&pair.value);
            previous_key = Some(&pair.key);
        }

        let key_bytes_len = u32::try_from(key_bytes.len())
            .map_err(|_| "nav_kv key bytes exceed u32".to_string())?;
        let value_bytes_len = u32::try_from(value_bytes.len())
            .map_err(|_| "nav_kv value bytes exceed u32".to_string())?;
        entries.push(Entry {
            key_offset: key_bytes_len,
            value_offset: value_bytes_len,
        });
        validate_nav_kv_parts(&entries, &key_bytes, page_size, value_bytes_len)?;

        let mut offset_table = Vec::with_capacity(entries.len() * ENTRY_LEN);
        for entry in &entries {
            push_u32(&mut offset_table, entry.key_offset);
            push_u32(&mut offset_table, entry.value_offset);
        }
        let offset_table_len = u32::try_from(offset_table.len())
            .map_err(|_| "nav_kv offset table exceeds u32".to_string())?;
        let offset_table_offset = 0u32;
        let key_table_offset = offset_table_len;
        let key_table_len = key_bytes_len;
        let value_table_offset = key_table_offset
            .checked_add(key_table_len)
            .ok_or_else(|| "nav_kv key/value table offsets overflow u32".to_string())?;
        let logical_bytes_len = value_table_offset
            .checked_add(value_bytes_len)
            .ok_or_else(|| "nav_kv logical bytes exceed u32".to_string())?;

        let mut logical_bytes = Vec::with_capacity(logical_bytes_len as usize);
        logical_bytes.extend_from_slice(&offset_table);
        logical_bytes.extend_from_slice(&key_bytes);
        logical_bytes.extend_from_slice(&value_bytes);

        let mut root_bytes = Vec::with_capacity(HEADER_LEN);
        root_bytes.extend_from_slice(MAGIC);
        push_u32(&mut root_bytes, VERSION);
        push_u32(
            &mut root_bytes,
            u32::try_from(pairs.len()).map_err(|_| "nav_kv entry count exceeds u32".to_string())?,
        );
        push_u32(&mut root_bytes, page_size);
        push_u32(&mut root_bytes, offset_table_offset);
        push_u32(&mut root_bytes, offset_table_len);
        push_u32(&mut root_bytes, key_table_offset);
        push_u32(&mut root_bytes, key_table_len);
        push_u32(&mut root_bytes, value_table_offset);
        push_u32(&mut root_bytes, value_bytes_len);
        push_u32(&mut root_bytes, logical_bytes_len);
        push_u32(&mut root_bytes, 0);
        while root_bytes.len() < HEADER_LEN {
            push_u32(&mut root_bytes, 0);
        }

        let page_size_usize = usize::try_from(page_size)
            .map_err(|_| "nav_kv page size does not fit usize".to_string())?;
        let pages = logical_bytes
            .chunks(page_size_usize)
            .map(|chunk| chunk.to_vec())
            .collect();

        Ok(NavKvBuildOutput {
            root_bytes,
            pages,
            page_size,
            logical_bytes_len,
            value_bytes_len,
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
            let offset_table_offset = read_u32(root_bytes, 28)?;
            let offset_table_len = read_u32(root_bytes, 32)?;
            let key_table_offset = read_u32(root_bytes, 36)?;
            let key_table_len = read_u32(root_bytes, 40)?;
            let value_table_offset = read_u32(root_bytes, 44)?;
            let value_bytes_len = read_u32(root_bytes, 48)?;
            let logical_bytes_len = read_u32(root_bytes, 52)?;
            if page_size == 0 {
                return Err("nav_kv page_size must be non-zero".to_string());
            }
            if root_bytes.len() != HEADER_LEN {
                return Err("nav_kv v2 root must be exactly the fixed header length".to_string());
            }
            let sentinel_count = entry_count
                .checked_add(1)
                .ok_or_else(|| "nav_kv entry count overflows u32".to_string())?;
            let expected_offset_table_len = sentinel_count
                .checked_mul(ENTRY_LEN as u32)
                .ok_or_else(|| "nav_kv offset table length overflows u32".to_string())?;
            if offset_table_offset != 0 {
                return Err("nav_kv offset table must start at logical offset 0".to_string());
            }
            if offset_table_len != expected_offset_table_len {
                return Err("nav_kv offset table length does not match entry count".to_string());
            }
            if key_table_offset != offset_table_offset + offset_table_len {
                return Err("nav_kv key table offset does not follow offset table".to_string());
            }
            if value_table_offset != key_table_offset + key_table_len {
                return Err("nav_kv value table offset does not follow key table".to_string());
            }
            if logical_bytes_len != value_table_offset + value_bytes_len {
                return Err("nav_kv logical length does not match table lengths".to_string());
            }
            Ok(Self {
                entry_count,
                page_size,
                offset_table_offset,
                offset_table_len,
                key_table_offset,
                key_table_len,
                value_table_offset,
                value_bytes_len,
                logical_bytes_len,
            })
        }

        pub fn len(&self) -> usize {
            self.entry_count as usize
        }

        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }

        pub fn page_count(&self) -> u32 {
            self.logical_bytes_len.div_ceil(self.page_size)
        }

        pub fn page_size(&self) -> u32 {
            self.page_size
        }

        pub fn logical_bytes_len(&self) -> u32 {
            self.logical_bytes_len
        }

        pub fn value_bytes_len(&self) -> u32 {
            self.value_bytes_len
        }

        pub fn get_value_range<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            key: &str,
            mut page_provider: P,
        ) -> Option<(u32, u32)> {
            let mut left = 0usize;
            let mut right = self.len();
            let target = key.as_bytes();
            while left < right {
                let mid = left + (right - left) / 2;
                let key_at_mid = self.key_at(mid, &mut page_provider)?;
                match key_at_mid.as_slice().cmp(target) {
                    Ordering::Less => left = mid + 1,
                    Ordering::Equal => return self.value_range_at(mid, page_provider),
                    Ordering::Greater => right = mid,
                }
            }
            None
        }

        pub fn prefix_keys<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            prefix: &str,
            mut page_provider: P,
        ) -> Option<Vec<String>> {
            let prefix = prefix.as_bytes();
            let mut left = 0usize;
            let mut right = self.len();
            while left < right {
                let mid = left + (right - left) / 2;
                let key = self.key_at(mid, &mut page_provider)?;
                if key.as_slice() < prefix {
                    left = mid + 1;
                } else {
                    right = mid;
                }
            }
            let mut out = Vec::new();
            let mut index = left;
            while index < self.len() {
                let key = self.key_at(index, &mut page_provider)?;
                if !key.starts_with(prefix) {
                    break;
                }
                out.push(String::from_utf8_lossy(&key).into_owned());
                index += 1;
            }
            Some(out)
        }

        pub fn extract_value<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            key: &str,
            mut page_provider: P,
        ) -> Option<Vec<u8>> {
            let (start, end) = self.get_value_range(key, &mut page_provider)?;
            if start == end {
                return None;
            }
            self.read_logical_range(self.value_table_offset + start, end - start, page_provider)
        }

        fn key_at<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            index: usize,
            mut page_provider: P,
        ) -> Option<Vec<u8>> {
            let (start, end) = self.key_range_at(index, &mut page_provider)?;
            self.read_logical_range(self.key_table_offset + start, end - start, page_provider)
        }

        fn key_range_at<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            index: usize,
            page_provider: P,
        ) -> Option<(u32, u32)> {
            let (current, next) = self.entry_pair_at(index, page_provider)?;
            Some((current.key_offset, next.key_offset))
        }

        fn value_range_at<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            index: usize,
            page_provider: P,
        ) -> Option<(u32, u32)> {
            let (current, next) = self.entry_pair_at(index, page_provider)?;
            Some((current.value_offset, next.value_offset))
        }

        fn entry_pair_at<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            index: usize,
            page_provider: P,
        ) -> Option<(Entry, Entry)> {
            if index >= self.len() {
                return None;
            }
            let offset = self.offset_table_offset + (index as u32) * ENTRY_LEN as u32;
            let bytes = self.read_logical_range(offset, (ENTRY_LEN * 2) as u32, page_provider)?;
            Some((
                Entry {
                    key_offset: read_u32(&bytes, 0).ok()?,
                    value_offset: read_u32(&bytes, 4).ok()?,
                },
                Entry {
                    key_offset: read_u32(&bytes, 8).ok()?,
                    value_offset: read_u32(&bytes, 12).ok()?,
                },
            ))
        }

        fn read_logical_range<P: FnMut(u32) -> Option<Vec<u8>>>(
            &self,
            start: u32,
            len: u32,
            mut page_provider: P,
        ) -> Option<Vec<u8>> {
            if len == 0 {
                return Some(Vec::new());
            }
            let end = start.checked_add(len)?;
            if end > self.logical_bytes_len {
                return None;
            }
            let start_page = start / self.page_size;
            let end_page = (end - 1) / self.page_size;
            let mut out = Vec::with_capacity(len as usize);
            for page_index in start_page..=end_page {
                let page = page_provider(page_index)?;
                let page_start = page_index * self.page_size;
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

    fn validate_nav_kv_parts(
        entries: &[Entry],
        key_bytes: &[u8],
        page_size: u32,
        value_bytes_len: u32,
    ) -> Result<(), String> {
        if page_size == 0 {
            return Err("nav_kv page_size must be non-zero".to_string());
        }
        if entries.len() < 2 {
            return Err("nav_kv needs at least one real entry plus sentinel".to_string());
        }
        let sentinel = entries.last().expect("checked non-empty");
        if sentinel.key_offset as usize != key_bytes.len() {
            return Err("nav_kv sentinel key offset must equal key_bytes_len".to_string());
        }
        if sentinel.value_offset != value_bytes_len {
            return Err("nav_kv sentinel value offset must equal value_bytes_len".to_string());
        }
        let mut seen = HashSet::new();
        for index in 0..entries.len() - 1 {
            let current = &entries[index];
            let next = &entries[index + 1];
            if current.key_offset >= next.key_offset {
                return Err("nav_kv key offsets must be strictly increasing".to_string());
            }
            if current.value_offset >= next.value_offset {
                return Err("nav_kv values must be non-empty and increasing".to_string());
            }
            if next.key_offset as usize > key_bytes.len() {
                return Err("nav_kv key offset exceeds key bytes length".to_string());
            }
            if next.value_offset > value_bytes_len {
                return Err("nav_kv value offset exceeds value bytes length".to_string());
            }
            let key = &key_bytes[current.key_offset as usize..next.key_offset as usize];
            if index > 0 {
                let previous = &entries[index - 1];
                let previous_key =
                    &key_bytes[previous.key_offset as usize..current.key_offset as usize];
                if previous_key >= key {
                    return Err("nav_kv keys must be strictly sorted".to_string());
                }
            }
            if !seen.insert(key.to_vec()) {
                return Err("duplicate nav_kv key".to_string());
            }
        }
        Ok(())
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
        let chunk = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| "nav_kv read past end of root".to_string())?;
        Ok(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::HashMap;

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
                8,
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
            let built =
                build_nav_kv_sorted(vec![pair("chart/catalog", "{}")], 8).expect("build nav kv");
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
        fn prefix_lookup_skips_sentinel() {
            let built = build_nav_kv_sorted(
                vec![
                    pair("waypoint/id/KRDD", "1"),
                    pair("waypoint/id/KRNT", "2"),
                    pair("waypoint/suggest/KR", "3"),
                ],
                8,
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
        fn value_can_cross_page_boundaries() {
            let built =
                build_nav_kv_sorted(vec![pair("k", "abcdefghijklmnop")], 5).expect("build nav kv");
            assert_eq!(built.pages.len(), 7);
            let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
            let value = root
                .extract_value("k", |page| built.pages.get(page as usize).cloned())
                .expect("value");
            assert_eq!(value, b"abcdefghijklmnop");
        }

        #[test]
        fn last_key_uses_sentinel_offsets_even_across_page_boundary() {
            let built =
                build_nav_kv_sorted(vec![pair("a", "1"), pair("b", "2"), pair("c", "last")], 23)
                    .expect("build nav kv");
            let root = NavKvRoot::parse(&built.root_bytes).expect("parse root");
            assert_eq!(
                root.extract_value("c", |page| built.pages.get(page as usize).cloned()),
                Some(b"last".to_vec())
            );
        }

        #[test]
        fn strict_builder_rejects_unsorted_keys() {
            let err = build_nav_kv_strict(vec![pair("b", "1"), pair("a", "2")], 8)
                .expect_err("unsorted keys should fail");
            assert!(err.contains("not sorted"));
        }

        #[test]
        fn builder_rejects_duplicate_keys() {
            let err = build_nav_kv_sorted(vec![pair("a", "1"), pair("a", "2")], 8)
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
                8,
            )
            .expect_err("empty value should fail");
            assert!(err.contains("must not be empty"));
        }

        #[test]
        fn parser_rejects_bad_magic() {
            let built = build_nav_kv_sorted(vec![pair("a", "1")], 8).expect("build nav kv");
            let mut root = built.root_bytes;
            root[0] = b'X';
            let err = NavKvRoot::parse(&root).expect_err("bad magic should fail");
            assert!(err.contains("invalid magic"));
        }

        #[test]
        fn repeated_extraction_can_reuse_cached_pages() {
            let built = build_nav_kv_sorted(vec![pair("a", "abcde"), pair("b", "fghij")], 5)
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
                        lat_max: 71.0,
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
                lat_min: 15.0,
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

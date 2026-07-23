use std::collections::{BTreeMap, BTreeSet};

use product_contracts::{
    AirportNotamEffect, LIVE_FEEDS_SCHEMA_VERSION, NOTAM_LIVE_FEED_CONTRACT_VERSION,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

pub const NOTAM_PRODUCT_ID: &str = "notams";
pub const NOTAM_MERKLE_BUCKET_COUNT: usize = 1024;
pub const NOTAM_MERKLE_GROUP_COUNT: usize = 32;
pub const NOTAM_MERKLE_BUCKETS_PER_GROUP: usize =
    NOTAM_MERKLE_BUCKET_COUNT / NOTAM_MERKLE_GROUP_COUNT;

const LEAF_DOMAIN: &[u8] = b"aerobag/notams/leaf/v1\0";
const BUCKET_DOMAIN: &[u8] = b"aerobag/notams/bucket/v1\0";
const GROUP_DOMAIN: &[u8] = b"aerobag/notams/group/v1\0";
const STATE_DOMAIN: &[u8] = b"aerobag/notams/state/v1\0";

pub type NotamHash = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamRecord {
    pub id: String,
    #[serde(default)]
    pub airport_id: Option<String>,
    #[serde(default)]
    pub airport_effects: BTreeSet<AirportNotamEffect>,
    #[serde(default)]
    pub notam_keyword: Option<String>,
    #[serde(default)]
    pub effective_start_utc: Option<String>,
    #[serde(default)]
    pub effective_end_utc: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub local_text: Option<String>,
    #[serde(default)]
    pub icao_text: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamCounters {
    pub notam_count: u64,
    pub airport_notam_count: u64,
    pub airport_notams_with_multiple_effects: u64,
    pub airport_notams_with_other_effect: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotamMutation {
    Upsert { record: NotamRecord },
    Remove { notam_id: String },
}

#[derive(Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum HumanReadableNotamMutation<'a> {
    Upsert { record: &'a NotamRecord },
    Remove { notam_id: &'a str },
}

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum OwnedHumanReadableNotamMutation {
    Upsert { record: NotamRecord },
    Remove { notam_id: String },
}

#[derive(Serialize)]
enum CompactNotamMutation<'a> {
    Upsert { record: &'a NotamRecord },
    Remove { notam_id: &'a str },
}

#[derive(Deserialize)]
enum OwnedCompactNotamMutation {
    Upsert { record: NotamRecord },
    Remove { notam_id: String },
}

impl Serialize for NotamMutation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            match self {
                Self::Upsert { record } => {
                    HumanReadableNotamMutation::Upsert { record }.serialize(serializer)
                }
                Self::Remove { notam_id } => HumanReadableNotamMutation::Remove {
                    notam_id: notam_id.as_str(),
                }
                .serialize(serializer),
            }
        } else {
            match self {
                Self::Upsert { record } => {
                    CompactNotamMutation::Upsert { record }.serialize(serializer)
                }
                Self::Remove { notam_id } => CompactNotamMutation::Remove {
                    notam_id: notam_id.as_str(),
                }
                .serialize(serializer),
            }
        }
    }
}

impl<'de> Deserialize<'de> for NotamMutation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            Ok(
                match OwnedHumanReadableNotamMutation::deserialize(deserializer)? {
                    OwnedHumanReadableNotamMutation::Upsert { record } => Self::Upsert { record },
                    OwnedHumanReadableNotamMutation::Remove { notam_id } => {
                        Self::Remove { notam_id }
                    }
                },
            )
        } else {
            Ok(
                match OwnedCompactNotamMutation::deserialize(deserializer)? {
                    OwnedCompactNotamMutation::Upsert { record } => Self::Upsert { record },
                    OwnedCompactNotamMutation::Remove { notam_id } => Self::Remove { notam_id },
                },
            )
        }
    }
}

impl NotamMutation {
    pub fn notam_id(&self) -> &str {
        match self {
            Self::Upsert { record } => &record.id,
            Self::Remove { notam_id } => notam_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamCheckpoint {
    pub schema_version: u32,
    pub product: String,
    pub contract_version: u32,
    pub state_id: String,
    pub counters: NotamCounters,
    pub records: Vec<NotamRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotamDelta {
    pub schema_version: u32,
    pub product: String,
    pub contract_version: u32,
    pub from_state_id: String,
    pub to_state_id: String,
    pub counters: NotamCounters,
    pub mutations: Vec<NotamMutation>,
}

impl NotamCheckpoint {
    pub fn new(state_id: String, counters: NotamCounters, records: Vec<NotamRecord>) -> Self {
        Self {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: NOTAM_PRODUCT_ID.to_string(),
            contract_version: NOTAM_LIVE_FEED_CONTRACT_VERSION,
            state_id,
            counters,
            records,
        }
    }

    pub fn validate_contract(&self) -> Result<(), NotamStateError> {
        validate_contract(self.schema_version, &self.product, self.contract_version)
    }
}

impl NotamDelta {
    pub fn new(
        from_state_id: String,
        to_state_id: String,
        counters: NotamCounters,
        mutations: Vec<NotamMutation>,
    ) -> Self {
        Self {
            schema_version: LIVE_FEEDS_SCHEMA_VERSION,
            product: NOTAM_PRODUCT_ID.to_string(),
            contract_version: NOTAM_LIVE_FEED_CONTRACT_VERSION,
            from_state_id,
            to_state_id,
            counters,
            mutations,
        }
    }

    pub fn validate_contract(&self) -> Result<(), NotamStateError> {
        validate_contract(self.schema_version, &self.product, self.contract_version)
    }
}

fn validate_contract(
    schema_version: u32,
    product: &str,
    contract_version: u32,
) -> Result<(), NotamStateError> {
    if schema_version != LIVE_FEEDS_SCHEMA_VERSION {
        return Err(NotamStateError::Contract(format!(
            "unsupported live-feed schema {schema_version}; expected {LIVE_FEEDS_SCHEMA_VERSION}"
        )));
    }
    if product != NOTAM_PRODUCT_ID {
        return Err(NotamStateError::Contract(format!(
            "NOTAM payload declares product {product}"
        )));
    }
    if contract_version != NOTAM_LIVE_FEED_CONTRACT_VERSION {
        return Err(NotamStateError::Contract(format!(
            "unsupported NOTAM contract {contract_version}; expected {NOTAM_LIVE_FEED_CONTRACT_VERSION}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotamApplyWork {
    pub mutations_applied: u64,
    pub canonical_record_lookups: u64,
    pub secondary_index_removals: u64,
    pub secondary_index_insertions: u64,
    pub leaf_hashes_computed: u64,
    pub bucket_records_hashed: u64,
    pub bucket_hashes_computed: u64,
    pub group_hashes_computed: u64,
    pub roots_computed: u64,
    pub full_record_collection_iterations: u64,
    pub full_state_serializations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotamStateError {
    Contract(String),
    Encoding(String),
    InvalidRecord(String),
    InvalidOrdering(String),
    BaseStateMismatch {
        expected: String,
        actual: String,
    },
    TargetStateMismatch {
        expected: String,
        actual: String,
    },
    CounterMismatch {
        expected: NotamCounters,
        actual: NotamCounters,
    },
    Invariant(String),
}

impl std::fmt::Display for NotamStateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(message)
            | Self::Encoding(message)
            | Self::InvalidRecord(message)
            | Self::InvalidOrdering(message)
            | Self::Invariant(message) => formatter.write_str(message),
            Self::BaseStateMismatch { expected, actual } => write!(
                formatter,
                "NOTAM delta starts at {expected}, but local state is {actual}"
            ),
            Self::TargetStateMismatch { expected, actual } => write!(
                formatter,
                "NOTAM target state mismatch: expected {expected}, computed {actual}"
            ),
            Self::CounterMismatch { expected, actual } => write!(
                formatter,
                "NOTAM counter mismatch: expected {expected:?}, computed {actual:?}"
            ),
        }
    }
}

impl std::error::Error for NotamStateError {}

#[derive(Debug, PartialEq, Eq)]
pub struct NotamState {
    state_id: String,
    state_hash: NotamHash,
    counters: NotamCounters,
    records: BTreeMap<String, NotamRecord>,
    by_airport: BTreeMap<String, Vec<String>>,
    merkle: NotamMerkleIndex,
}

impl Default for NotamState {
    fn default() -> Self {
        Self::empty()
    }
}

impl NotamState {
    pub fn empty() -> Self {
        let merkle = NotamMerkleIndex::empty();
        let counters = NotamCounters::default();
        let state_hash = state_hash(&merkle.group_hashes, counters);
        Self {
            state_id: hash_hex(&state_hash),
            state_hash,
            counters,
            records: BTreeMap::new(),
            by_airport: BTreeMap::new(),
            merkle,
        }
    }

    pub fn from_checkpoint(
        checkpoint: NotamCheckpoint,
        work: &mut NotamApplyWork,
    ) -> Result<Self, NotamStateError> {
        checkpoint.validate_contract()?;
        validate_record_order(&checkpoint.records)?;
        let mut state = Self::empty();
        for record in checkpoint.records {
            state.apply_mutation(NotamMutation::Upsert { record }, work)?;
        }
        state.require_target(&checkpoint.state_id, checkpoint.counters)?;
        Ok(state)
    }

    pub fn apply_delta(
        &mut self,
        delta: NotamDelta,
        work: &mut NotamApplyWork,
    ) -> Result<(), NotamStateError> {
        delta.validate_contract()?;
        if self.state_id != delta.from_state_id {
            return Err(NotamStateError::BaseStateMismatch {
                expected: delta.from_state_id,
                actual: self.state_id.clone(),
            });
        }
        validate_mutation_order(&delta.mutations)?;
        for mutation in delta.mutations {
            self.apply_mutation(mutation, work)?;
        }
        self.require_target(&delta.to_state_id, delta.counters)
    }

    pub fn apply_mutation(
        &mut self,
        mutation: NotamMutation,
        work: &mut NotamApplyWork,
    ) -> Result<(), NotamStateError> {
        let notam_id = mutation.notam_id().to_string();
        validate_notam_id(&notam_id)?;
        work.mutations_applied += 1;
        work.canonical_record_lookups += 1;
        let old_record = self.records.remove(&notam_id);
        if let Some(record) = old_record.as_ref() {
            self.remove_from_airport_index(record, work)?;
            subtract_record_counters(&mut self.counters, record)?;
        }

        let bucket = bucket_for_id(&notam_id);
        match mutation {
            NotamMutation::Upsert { record } => {
                if record.id != notam_id {
                    return Err(NotamStateError::InvalidRecord(format!(
                        "NOTAM upsert ID {} does not match mutation ID {notam_id}",
                        record.id
                    )));
                }
                let canonical = canonical_record_bytes(&record)?;
                let leaf = leaf_hash(&notam_id, &canonical);
                work.leaf_hashes_computed += 1;
                self.merkle.bucket_members[bucket].insert(notam_id.clone(), leaf);
                add_record_counters(&mut self.counters, &record);
                self.insert_into_airport_index(&record, work)?;
                self.records.insert(notam_id, record);
            }
            NotamMutation::Remove { .. } => {
                self.merkle.bucket_members[bucket].remove(&notam_id);
            }
        }

        self.recompute_after_bucket(bucket, work);
        Ok(())
    }

    pub fn state_id(&self) -> &str {
        &self.state_id
    }

    pub fn counters(&self) -> NotamCounters {
        self.counters
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn record(&self, notam_id: &str) -> Option<&NotamRecord> {
        self.records.get(notam_id)
    }

    pub fn airport_records(&self, airport_id: &str) -> Vec<&NotamRecord> {
        let normalized = normalize_airport_id(airport_id);
        self.by_airport
            .get(&normalized)
            .into_iter()
            .flatten()
            .filter_map(|id| self.records.get(id))
            .collect()
    }

    pub fn canonical_records(&self) -> impl Iterator<Item = (&str, &NotamRecord)> {
        self.records
            .iter()
            .map(|(id, record)| (id.as_str(), record))
    }

    pub fn airport_index(&self) -> &BTreeMap<String, Vec<String>> {
        &self.by_airport
    }

    pub fn checkpoint(&self) -> NotamCheckpoint {
        NotamCheckpoint::new(
            self.state_id.clone(),
            self.counters,
            self.records.values().cloned().collect(),
        )
    }

    pub fn fully_recomputed_state_id(&self) -> Result<String, NotamStateError> {
        recompute_record_identity(self.records.values()).map(|(state_id, _)| state_id)
    }

    fn require_target(
        &self,
        expected_state_id: &str,
        expected_counters: NotamCounters,
    ) -> Result<(), NotamStateError> {
        if self.counters != expected_counters {
            return Err(NotamStateError::CounterMismatch {
                expected: expected_counters,
                actual: self.counters,
            });
        }
        if self.state_id != expected_state_id {
            return Err(NotamStateError::TargetStateMismatch {
                expected: expected_state_id.to_string(),
                actual: self.state_id.clone(),
            });
        }
        Ok(())
    }

    fn insert_into_airport_index(
        &mut self,
        record: &NotamRecord,
        work: &mut NotamApplyWork,
    ) -> Result<(), NotamStateError> {
        let Some(airport_id) = record.airport_id.as_deref() else {
            return Ok(());
        };
        let airport_id = normalize_airport_id(airport_id);
        if airport_id.is_empty() {
            return Ok(());
        }
        let ids = self.by_airport.entry(airport_id).or_default();
        match ids.binary_search(&record.id) {
            Ok(_) => Err(NotamStateError::Invariant(format!(
                "NOTAM {} is already present in its airport index",
                record.id
            ))),
            Err(index) => {
                ids.insert(index, record.id.clone());
                work.secondary_index_insertions += 1;
                Ok(())
            }
        }
    }

    fn remove_from_airport_index(
        &mut self,
        record: &NotamRecord,
        work: &mut NotamApplyWork,
    ) -> Result<(), NotamStateError> {
        let Some(airport_id) = record.airport_id.as_deref() else {
            return Ok(());
        };
        let airport_id = normalize_airport_id(airport_id);
        if airport_id.is_empty() {
            return Ok(());
        }
        let remove_airport = {
            let ids = self.by_airport.get_mut(&airport_id).ok_or_else(|| {
                NotamStateError::Invariant(format!(
                    "NOTAM {} is missing airport index {airport_id}",
                    record.id
                ))
            })?;
            let index = ids.binary_search(&record.id).map_err(|_| {
                NotamStateError::Invariant(format!(
                    "NOTAM {} is missing from airport index {airport_id}",
                    record.id
                ))
            })?;
            ids.remove(index);
            work.secondary_index_removals += 1;
            ids.is_empty()
        };
        if remove_airport {
            self.by_airport.remove(&airport_id);
        }
        Ok(())
    }

    fn recompute_after_bucket(&mut self, bucket: usize, work: &mut NotamApplyWork) {
        let members = &self.merkle.bucket_members[bucket];
        work.bucket_records_hashed += members.len() as u64;
        self.merkle.bucket_hashes[bucket] = bucket_hash(bucket, members);
        work.bucket_hashes_computed += 1;

        let group = bucket / NOTAM_MERKLE_BUCKETS_PER_GROUP;
        self.merkle.group_hashes[group] = group_hash(group, &self.merkle.bucket_hashes);
        work.group_hashes_computed += 1;
        self.state_hash = state_hash(&self.merkle.group_hashes, self.counters);
        self.state_id = hash_hex(&self.state_hash);
        work.roots_computed += 1;
    }
}

pub fn recompute_checkpoint_identity(
    records: &[NotamRecord],
) -> Result<(String, NotamCounters), NotamStateError> {
    validate_record_order(records)?;
    recompute_record_identity(records.iter())
}

fn recompute_record_identity<'a>(
    records: impl IntoIterator<Item = &'a NotamRecord>,
) -> Result<(String, NotamCounters), NotamStateError> {
    let mut merkle = NotamMerkleIndex::empty();
    let mut counters = NotamCounters::default();
    for record in records {
        let canonical = canonical_record_bytes(record)?;
        merkle.bucket_members[bucket_for_id(&record.id)]
            .insert(record.id.clone(), leaf_hash(&record.id, &canonical));
        add_record_counters(&mut counters, record);
    }
    merkle.recompute_all();
    Ok((
        hash_hex(&state_hash(&merkle.group_hashes, counters)),
        counters,
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NotamMerkleIndex {
    bucket_members: Vec<BTreeMap<String, NotamHash>>,
    bucket_hashes: Vec<NotamHash>,
    group_hashes: Vec<NotamHash>,
}

impl NotamMerkleIndex {
    fn empty() -> Self {
        let bucket_members = (0..NOTAM_MERKLE_BUCKET_COUNT)
            .map(|_| BTreeMap::new())
            .collect::<Vec<_>>();
        let bucket_hashes = bucket_members
            .iter()
            .enumerate()
            .map(|(bucket, members)| bucket_hash(bucket, members))
            .collect::<Vec<_>>();
        let group_hashes = (0..NOTAM_MERKLE_GROUP_COUNT)
            .map(|group| group_hash(group, &bucket_hashes))
            .collect();
        Self {
            bucket_members,
            bucket_hashes,
            group_hashes,
        }
    }

    fn recompute_all(&mut self) {
        for bucket in 0..NOTAM_MERKLE_BUCKET_COUNT {
            self.bucket_hashes[bucket] = bucket_hash(bucket, &self.bucket_members[bucket]);
        }
        for group in 0..NOTAM_MERKLE_GROUP_COUNT {
            self.group_hashes[group] = group_hash(group, &self.bucket_hashes);
        }
    }
}

pub fn canonical_record_bytes(record: &NotamRecord) -> Result<Vec<u8>, NotamStateError> {
    validate_notam_id(&record.id)?;
    serde_json::to_vec(record).map_err(|error| {
        NotamStateError::Encoding(format!(
            "failed to encode canonical NOTAM {}: {error}",
            record.id
        ))
    })
}

pub fn validate_mutation_order(mutations: &[NotamMutation]) -> Result<(), NotamStateError> {
    validate_id_order(mutations.iter().map(NotamMutation::notam_id), "mutation")
}

pub fn validate_record_order(records: &[NotamRecord]) -> Result<(), NotamStateError> {
    validate_id_order(
        records.iter().map(|record| record.id.as_str()),
        "checkpoint record",
    )
}

fn validate_id_order<'a>(
    ids: impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<(), NotamStateError> {
    let mut previous = None;
    for id in ids {
        validate_notam_id(id)?;
        if let Some(previous) = previous {
            if previous >= id {
                return Err(NotamStateError::InvalidOrdering(format!(
                    "NOTAM {label} IDs are not strictly ordered: {previous:?} then {id:?}"
                )));
            }
        }
        previous = Some(id);
    }
    Ok(())
}

fn validate_notam_id(id: &str) -> Result<(), NotamStateError> {
    if id.is_empty() || id.trim() != id {
        return Err(NotamStateError::InvalidRecord(format!(
            "invalid NOTAM ID {id:?}"
        )));
    }
    Ok(())
}

fn normalize_airport_id(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn record_counter_contribution(record: &NotamRecord) -> NotamCounters {
    let airport = record
        .airport_id
        .as_deref()
        .map(normalize_airport_id)
        .is_some_and(|value| !value.is_empty());
    NotamCounters {
        notam_count: 1,
        airport_notam_count: u64::from(airport),
        airport_notams_with_multiple_effects: u64::from(
            airport && record.airport_effects.len() > 1,
        ),
        airport_notams_with_other_effect: u64::from(
            airport && record.airport_effects.contains(&AirportNotamEffect::Other),
        ),
    }
}

fn add_record_counters(counters: &mut NotamCounters, record: &NotamRecord) {
    let contribution = record_counter_contribution(record);
    counters.notam_count += contribution.notam_count;
    counters.airport_notam_count += contribution.airport_notam_count;
    counters.airport_notams_with_multiple_effects +=
        contribution.airport_notams_with_multiple_effects;
    counters.airport_notams_with_other_effect += contribution.airport_notams_with_other_effect;
}

fn subtract_record_counters(
    counters: &mut NotamCounters,
    record: &NotamRecord,
) -> Result<(), NotamStateError> {
    let contribution = record_counter_contribution(record);
    counters.notam_count = counters
        .notam_count
        .checked_sub(contribution.notam_count)
        .ok_or_else(|| NotamStateError::Invariant("NOTAM count underflow".to_string()))?;
    counters.airport_notam_count = counters
        .airport_notam_count
        .checked_sub(contribution.airport_notam_count)
        .ok_or_else(|| NotamStateError::Invariant("airport NOTAM count underflow".to_string()))?;
    counters.airport_notams_with_multiple_effects = counters
        .airport_notams_with_multiple_effects
        .checked_sub(contribution.airport_notams_with_multiple_effects)
        .ok_or_else(|| {
            NotamStateError::Invariant("multiple-effect NOTAM count underflow".to_string())
        })?;
    counters.airport_notams_with_other_effect = counters
        .airport_notams_with_other_effect
        .checked_sub(contribution.airport_notams_with_other_effect)
        .ok_or_else(|| {
            NotamStateError::Invariant("other-effect NOTAM count underflow".to_string())
        })?;
    Ok(())
}

pub fn bucket_for_id(id: &str) -> usize {
    let hash = Sha256::digest(id.as_bytes());
    (((hash[0] as usize) << 2) | ((hash[1] as usize) >> 6)) & (NOTAM_MERKLE_BUCKET_COUNT - 1)
}

pub fn record_leaf_hash(record: &NotamRecord) -> Result<NotamHash, NotamStateError> {
    let canonical = canonical_record_bytes(record)?;
    Ok(leaf_hash(&record.id, &canonical))
}

fn leaf_hash(id: &str, canonical_record: &[u8]) -> NotamHash {
    let mut hasher = Sha256::new();
    hasher.update(LEAF_DOMAIN);
    update_frame(&mut hasher, id.as_bytes());
    update_frame(&mut hasher, canonical_record);
    hasher.finalize().into()
}

pub fn compute_bucket_hash<'a>(
    bucket: usize,
    members: impl IntoIterator<Item = (&'a str, &'a NotamHash)>,
) -> NotamHash {
    let mut hasher = Sha256::new();
    hasher.update(BUCKET_DOMAIN);
    hasher.update((bucket as u16).to_be_bytes());
    let members = members.into_iter().collect::<Vec<_>>();
    hasher.update((members.len() as u32).to_be_bytes());
    for (id, leaf) in members {
        update_frame(&mut hasher, id.as_bytes());
        hasher.update(leaf);
    }
    hasher.finalize().into()
}

fn bucket_hash(bucket: usize, members: &BTreeMap<String, NotamHash>) -> NotamHash {
    compute_bucket_hash(bucket, members.iter().map(|(id, hash)| (id.as_str(), hash)))
}

pub fn compute_group_hash(
    group: usize,
    bucket_hashes: &[NotamHash],
) -> Result<NotamHash, NotamStateError> {
    if group >= NOTAM_MERKLE_GROUP_COUNT || bucket_hashes.len() != NOTAM_MERKLE_BUCKET_COUNT {
        return Err(NotamStateError::Invariant(format!(
            "invalid NOTAM Merkle group input: group={group} bucket_hashes={}",
            bucket_hashes.len()
        )));
    }
    Ok(group_hash(group, bucket_hashes))
}

fn group_hash(group: usize, bucket_hashes: &[NotamHash]) -> NotamHash {
    let mut hasher = Sha256::new();
    hasher.update(GROUP_DOMAIN);
    hasher.update([group as u8]);
    let start = group * NOTAM_MERKLE_BUCKETS_PER_GROUP;
    for hash in &bucket_hashes[start..start + NOTAM_MERKLE_BUCKETS_PER_GROUP] {
        hasher.update(hash);
    }
    hasher.finalize().into()
}

pub fn compute_state_id(
    group_hashes: &[NotamHash],
    counters: NotamCounters,
) -> Result<String, NotamStateError> {
    if group_hashes.len() != NOTAM_MERKLE_GROUP_COUNT {
        return Err(NotamStateError::Invariant(format!(
            "invalid NOTAM Merkle root input: group_hashes={}",
            group_hashes.len()
        )));
    }
    Ok(hash_hex(&state_hash(group_hashes, counters)))
}

pub fn empty_merkle_hashes() -> (Vec<NotamHash>, Vec<NotamHash>) {
    let members = BTreeMap::new();
    let bucket_hashes = (0..NOTAM_MERKLE_BUCKET_COUNT)
        .map(|bucket| bucket_hash(bucket, &members))
        .collect::<Vec<_>>();
    let group_hashes = (0..NOTAM_MERKLE_GROUP_COUNT)
        .map(|group| group_hash(group, &bucket_hashes))
        .collect::<Vec<_>>();
    (bucket_hashes, group_hashes)
}

fn state_hash(group_hashes: &[NotamHash], counters: NotamCounters) -> NotamHash {
    let mut hasher = Sha256::new();
    hasher.update(STATE_DOMAIN);
    hasher.update(LIVE_FEEDS_SCHEMA_VERSION.to_be_bytes());
    hasher.update(NOTAM_LIVE_FEED_CONTRACT_VERSION.to_be_bytes());
    hasher.update(counters.notam_count.to_be_bytes());
    hasher.update(counters.airport_notam_count.to_be_bytes());
    hasher.update(counters.airport_notams_with_multiple_effects.to_be_bytes());
    hasher.update(counters.airport_notams_with_other_effect.to_be_bytes());
    for hash in group_hashes {
        hasher.update(hash);
    }
    hasher.finalize().into()
}

fn update_frame(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

pub fn hash_hex(hash: &NotamHash) -> String {
    let mut text = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write;
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: &str, airport_id: Option<&str>, text: &str) -> NotamRecord {
        NotamRecord {
            id: id.to_string(),
            airport_id: airport_id.map(str::to_string),
            airport_effects: BTreeSet::from([AirportNotamEffect::RoutineAdvisory]),
            notam_keyword: Some("AD".to_string()),
            effective_start_utc: Some("2026-07-22T00:00:00Z".to_string()),
            effective_end_utc: None,
            text: Some(text.to_string()),
            local_text: None,
            icao_text: None,
        }
    }

    #[test]
    fn incremental_mutations_match_full_recomputation() {
        let mut state = NotamState::empty();
        let mut work = NotamApplyWork::default();
        for mutation in [
            NotamMutation::Upsert {
                record: record("A", Some("ksea"), "first"),
            },
            NotamMutation::Upsert {
                record: record("B", Some("KSEA"), "second"),
            },
            NotamMutation::Upsert {
                record: record("A", Some("KPAE"), "moved"),
            },
            NotamMutation::Remove {
                notam_id: "B".to_string(),
            },
        ] {
            state.apply_mutation(mutation, &mut work).unwrap();
            assert_eq!(state.state_id(), state.fully_recomputed_state_id().unwrap());
        }
        assert_eq!(state.airport_records("KSEA").len(), 0);
        assert_eq!(state.airport_records("kpae")[0].id, "A");
        assert_eq!(work.full_record_collection_iterations, 0);
        assert_eq!(work.full_state_serializations, 0);
    }

    #[test]
    fn checkpoint_and_delta_round_trip_exact_state() {
        let mut source = NotamState::empty();
        let mut work = NotamApplyWork::default();
        source
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record("A", Some("KSEA"), "first"),
                },
                &mut work,
            )
            .unwrap();
        let checkpoint = source.checkpoint();
        let mut installed =
            NotamState::from_checkpoint(checkpoint, &mut NotamApplyWork::default()).unwrap();

        let from = source.state_id().to_string();
        let mutation = NotamMutation::Upsert {
            record: record("B", Some("KPAE"), "second"),
        };
        source.apply_mutation(mutation.clone(), &mut work).unwrap();
        let delta = NotamDelta::new(
            from,
            source.state_id().to_string(),
            source.counters(),
            vec![mutation],
        );
        installed
            .apply_delta(delta, &mut NotamApplyWork::default())
            .unwrap();
        assert_eq!(installed.state_id(), source.state_id());
        assert_eq!(
            installed.canonical_records().collect::<Vec<_>>(),
            source.canonical_records().collect::<Vec<_>>()
        );
    }

    #[test]
    fn target_mismatch_leaves_detectably_untrusted_mutated_state() {
        let mut state = NotamState::empty();
        let delta = NotamDelta::new(
            state.state_id().to_string(),
            "0".repeat(64),
            NotamCounters {
                notam_count: 1,
                airport_notam_count: 1,
                ..Default::default()
            },
            vec![NotamMutation::Upsert {
                record: record("A", Some("KSEA"), "first"),
            }],
        );
        assert!(matches!(
            state.apply_delta(delta, &mut NotamApplyWork::default()),
            Err(NotamStateError::TargetStateMismatch { .. })
        ));
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn stale_base_is_rejected_without_mutation() {
        let mut state = NotamState::empty();
        let original = state.state_id().to_string();
        let delta = NotamDelta::new(
            "f".repeat(64),
            "e".repeat(64),
            NotamCounters::default(),
            Vec::new(),
        );
        assert!(matches!(
            state.apply_delta(delta, &mut NotamApplyWork::default()),
            Err(NotamStateError::BaseStateMismatch { .. })
        ));
        assert_eq!(state.state_id(), original);
        assert!(state.is_empty());
    }

    #[test]
    fn mutation_order_is_strict_and_unified_across_operation_types() {
        let mutations = vec![
            NotamMutation::Remove {
                notam_id: "B".to_string(),
            },
            NotamMutation::Upsert {
                record: record("A", None, "out of order"),
            },
        ];
        assert!(matches!(
            validate_mutation_order(&mutations),
            Err(NotamStateError::InvalidOrdering(_))
        ));
    }

    #[test]
    fn mutations_use_tagged_json_and_compact_postcard_encodings() {
        let mutations = vec![
            NotamMutation::Upsert {
                record: record("A", Some("KSEA"), "upsert"),
            },
            NotamMutation::Remove {
                notam_id: "B".to_string(),
            },
        ];
        let json = serde_json::to_value(&mutations).unwrap();
        assert_eq!(json[0]["operation"], "upsert");
        assert_eq!(json[1]["operation"], "remove");
        assert_eq!(
            serde_json::from_value::<Vec<NotamMutation>>(json).unwrap(),
            mutations
        );
        let postcard = postcard::to_allocvec(&mutations).unwrap();
        assert_eq!(
            postcard::from_bytes::<Vec<NotamMutation>>(&postcard).unwrap(),
            mutations
        );
    }

    #[test]
    fn canonical_record_encoding_is_stable() {
        let record = record("A", Some("KSEA"), "RWY 16L CLSD");
        assert_eq!(
            String::from_utf8(canonical_record_bytes(&record).unwrap()).unwrap(),
            r#"{"id":"A","airport_id":"KSEA","airport_effects":["routine_advisory"],"notam_keyword":"AD","effective_start_utc":"2026-07-22T00:00:00Z","effective_end_utc":null,"text":"RWY 16L CLSD","local_text":null,"icao_text":null}"#
        );
    }

    #[test]
    fn hash_contract_golden_vectors() {
        let record = record("A", Some("KSEA"), "RWY 16L CLSD");
        let (empty_buckets, empty_groups) = empty_merkle_hashes();
        let mut state = NotamState::empty();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record.clone(),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        assert_eq!(bucket_for_id(&record.id), 342);
        assert_eq!(
            hash_hex(&record_leaf_hash(&record).unwrap()),
            "f860df325f17dd90b4e060c7bc08b412ad7d022d427c554ef01f344ab7f99eb2"
        );
        assert_eq!(
            hash_hex(&empty_buckets[0]),
            "f9be1905883ffeac7d5adc9c111007cdfb725bbdee745ff16c764ec833c58769"
        );
        assert_eq!(
            hash_hex(&empty_groups[0]),
            "c5cfaac126d022917f20a5aed1a2afd44f17ff839c8ae4ca7643aa9e55926968"
        );
        assert_eq!(
            NotamState::empty().state_id(),
            "fb2c6aa035e553522c03fda807fe182729b568fb6d13148e009010efdf6f42fb"
        );
        assert_eq!(
            state.state_id(),
            "f69ba6b76f5de44e6a0c44c63cbb7002852a889b155d997a1abd1e0a25811f95"
        );
    }

    #[test]
    fn insertion_order_and_same_bucket_order_do_not_change_identity() {
        let mut records_by_bucket = BTreeMap::<usize, Vec<NotamRecord>>::new();
        let pair = (0..10_000)
            .find_map(|index| {
                let record = record(&format!("N{index:04}"), Some("KSEA"), "same bucket");
                let records = records_by_bucket
                    .entry(bucket_for_id(&record.id))
                    .or_default();
                records.push(record);
                (records.len() == 2).then(|| records.clone())
            })
            .expect("10,000 identifiers must contain a Merkle bucket collision");

        let build = |records: Vec<NotamRecord>| {
            let mut state = NotamState::empty();
            for record in records {
                state
                    .apply_mutation(
                        NotamMutation::Upsert { record },
                        &mut NotamApplyWork::default(),
                    )
                    .unwrap();
            }
            state
        };
        let forward = build(pair.clone());
        let reverse = build(pair.into_iter().rev().collect());
        assert_eq!(forward.state_id(), reverse.state_id());
        assert_eq!(
            forward.canonical_records().collect::<Vec<_>>(),
            reverse.canonical_records().collect::<Vec<_>>()
        );
    }

    #[test]
    fn restoring_original_content_restores_original_root() {
        let original_record = record("A", Some("KSEA"), "original");
        let mut state = NotamState::empty();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: original_record.clone(),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        let original_root = state.state_id().to_string();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record("A", Some("KPAE"), "changed"),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: original_record,
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        assert_eq!(state.state_id(), original_root);

        state
            .apply_mutation(
                NotamMutation::Remove {
                    notam_id: "A".to_string(),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        assert_eq!(state.state_id(), NotamState::empty().state_id());
    }

    #[test]
    fn one_record_update_recomputes_only_one_merkle_path() {
        let mut state = NotamState::empty();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record("A", Some("KSEA"), "original"),
                },
                &mut NotamApplyWork::default(),
            )
            .unwrap();
        let mut work = NotamApplyWork::default();
        state
            .apply_mutation(
                NotamMutation::Upsert {
                    record: record("A", Some("KPAE"), "changed"),
                },
                &mut work,
            )
            .unwrap();
        assert_eq!(work.mutations_applied, 1);
        assert_eq!(work.canonical_record_lookups, 1);
        assert_eq!(work.secondary_index_removals, 1);
        assert_eq!(work.secondary_index_insertions, 1);
        assert_eq!(work.leaf_hashes_computed, 1);
        assert_eq!(work.bucket_hashes_computed, 1);
        assert_eq!(work.group_hashes_computed, 1);
        assert_eq!(work.roots_computed, 1);
        assert_eq!(work.full_record_collection_iterations, 0);
        assert_eq!(work.full_state_serializations, 0);
    }

    #[test]
    fn long_incremental_sequence_matches_full_recomputation() {
        let mut state = NotamState::empty();
        let mut seed = 0x5eed_cafe_u64;
        for step in 0..2_000 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let index = (seed % 127) as usize;
            let id = format!("R{index:03}");
            let mutation = if seed & 3 == 0 {
                NotamMutation::Remove { notam_id: id }
            } else {
                NotamMutation::Upsert {
                    record: record(
                        &id,
                        Some(if seed & 4 == 0 { "KSEA" } else { "KPAE" }),
                        &format!("revision {step}"),
                    ),
                }
            };
            state
                .apply_mutation(mutation, &mut NotamApplyWork::default())
                .unwrap();
            assert_eq!(state.state_id(), state.fully_recomputed_state_id().unwrap());
        }
    }

    #[test]
    fn checkpoint_and_backlog_materialization_schedules_converge_exactly() {
        let mut initial = NotamState::empty();
        for index in 0..128 {
            initial
                .apply_mutation(
                    NotamMutation::Upsert {
                        record: record(
                            &format!("N{index:03}"),
                            Some(if index % 2 == 0 { "KSEA" } else { "KPAE" }),
                            "initial",
                        ),
                    },
                    &mut NotamApplyWork::default(),
                )
                .unwrap();
        }
        let mut mutations = Vec::new();
        let mut snapshots = vec![initial.checkpoint()];
        let mut source = initial;
        let mut seed = 0x51a7_e123_u64;
        for revision in 0..300 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let id = format!("N{:03}", seed % 160);
            let mutation = if seed.is_multiple_of(7) {
                NotamMutation::Remove { notam_id: id }
            } else {
                NotamMutation::Upsert {
                    record: record(
                        &id,
                        Some(if seed & 1 == 0 { "KSEA" } else { "KBFI" }),
                        &format!("revision {revision}"),
                    ),
                }
            };
            source
                .apply_mutation(mutation.clone(), &mut NotamApplyWork::default())
                .unwrap();
            mutations.push(mutation);
            snapshots.push(source.checkpoint());
        }
        let expected = snapshots.last().unwrap();

        let mut schedules = vec![(0_usize, 1_usize), (7, 7), (31, 31), (100, 100)];
        let mut schedule_seed = 0xa11c_e55e_u64;
        for _ in 0..256 {
            schedule_seed = schedule_seed
                .wrapping_mul(2862933555777941757)
                .wrapping_add(3037000493);
            schedules.push((
                (schedule_seed as usize) % snapshots.len(),
                ((schedule_seed >> 32) as usize % 37) + 1,
            ));
        }

        for (checkpoint_boundary, max_span) in schedules {
            let mut client = NotamState::from_checkpoint(
                snapshots[checkpoint_boundary].clone(),
                &mut NotamApplyWork::default(),
            )
            .unwrap();
            let mut boundary = checkpoint_boundary;
            let mut path_seed = (checkpoint_boundary as u64)
                .wrapping_mul(0x9e37_79b9)
                .wrapping_add(max_span as u64);
            while boundary < mutations.len() {
                path_seed = path_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let span = ((path_seed as usize) % max_span).saturating_add(1);
                let next = (boundary + span).min(mutations.len());
                let mut collapsed = BTreeMap::new();
                for mutation in &mutations[boundary..next] {
                    collapsed.insert(mutation.notam_id().to_string(), mutation.clone());
                }
                let delta = NotamDelta::new(
                    snapshots[boundary].state_id.clone(),
                    snapshots[next].state_id.clone(),
                    snapshots[next].counters,
                    collapsed.into_values().collect(),
                );
                client
                    .apply_delta(delta, &mut NotamApplyWork::default())
                    .unwrap();
                assert_eq!(client.state_id(), snapshots[next].state_id);
                assert_eq!(client.counters(), snapshots[next].counters);
                assert_eq!(
                    client
                        .canonical_records()
                        .map(|(_, record)| record)
                        .collect::<Vec<_>>(),
                    snapshots[next].records.iter().collect::<Vec<_>>()
                );
                boundary = next;
            }
            assert_eq!(client.state_id(), expected.state_id);
            assert_eq!(client.counters(), expected.counters);
            assert_eq!(
                client
                    .canonical_records()
                    .map(|(_, record)| record)
                    .collect::<Vec<_>>(),
                expected.records.iter().collect::<Vec<_>>()
            );
        }
    }
}

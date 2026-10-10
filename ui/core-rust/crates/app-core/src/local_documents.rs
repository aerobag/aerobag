// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Local documents are not cloud records. Core owns ordering, retries and errors;
//! a host only reads bytes and reports completion of atomic replacements.

use crate::{AppError, AppErrorKind, AppResult};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

pub const SESSION_DOCUMENT: &str = "aerobag.core.settings.v1";
pub const TOUR_DOCUMENT: &str = "aerobag.tour.introduction.v1";
const RETRY_MS: i64 = 30_000;

/// Existing Android filenames are part of the local persistence contract.
pub fn native_file_name(key: &str) -> String {
    match key {
        SESSION_DOCUMENT => "core-settings-v1.json".into(),
        TOUR_DOCUMENT => "tour-introduction-v1.json".into(),
        _ => format!("{key}.bin"),
    }
}

pub type DocumentCompletion = Box<dyn FnOnce(Result<(), String>) + Send>;

pub trait LocalDocumentBackend: Send + Sync {
    /// Startup reads run off the input thread (web supplies preloaded results).
    fn read(&self, key: &str) -> AppResult<Option<Vec<u8>>>;
    /// Must not block the caller. None deletes a document. Completion means the
    /// atomic storage operation finished, not merely that it was enqueued.
    fn write(&self, key: &str, bytes: Option<Vec<u8>>, complete: DocumentCompletion);
}
pub type LocalDocumentBackendHandle = Arc<dyn LocalDocumentBackend>;

#[derive(Clone, Default)]
struct Document {
    value: Option<Vec<u8>>,
    persisted: Option<Vec<u8>>,
    revision: u64,
    active: Option<u64>,
    error: Option<String>,
    read_failed: bool,
    retry_at: i64,
}

#[derive(Default)]
struct State {
    documents: BTreeMap<String, Document>,
    now: i64,
    refresh_at: Option<i64>,
}

pub struct LocalDocuments {
    backend: LocalDocumentBackendHandle,
    state: Mutex<State>,
}

impl LocalDocuments {
    pub fn new(backend: LocalDocumentBackendHandle) -> Arc<Self> {
        Arc::new(Self {
            backend,
            state: Mutex::new(State::default()),
        })
    }

    pub fn read(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
        let mut state = self.state.lock().unwrap();
        if !state.documents.contains_key(key) {
            let document = match self.backend.read(key) {
                Ok(value) => Document {
                    persisted: value.clone(),
                    value,
                    ..Document::default()
                },
                Err(error) => Document {
                    error: Some(error.message),
                    read_failed: true,
                    ..Document::default()
                },
            };
            state.documents.insert(key.to_owned(), document);
        }
        let document = &state.documents[key];
        if document.read_failed {
            return Err(storage_error(
                document.error.as_deref().unwrap_or("Read failed"),
            ));
        }
        Ok(document.value.clone())
    }

    pub fn replace(self: &Arc<Self>, key: &str, bytes: &[u8]) -> AppResult<()> {
        self.set(key, Some(bytes.to_vec()))
    }

    pub fn delete(self: &Arc<Self>, key: &str) -> AppResult<()> {
        self.set(key, None)
    }

    fn set(self: &Arc<Self>, key: &str, value: Option<Vec<u8>>) -> AppResult<()> {
        self.read(key)?;
        {
            let mut state = self.state.lock().unwrap();
            let document = state.documents.get_mut(key).unwrap();
            if document.value != value {
                document.value = value;
                document.revision += 1;
            }
        }
        self.pump(key);
        Ok(())
    }

    fn pump(self: &Arc<Self>, key: &str) {
        let (revision, value) = {
            let mut state = self.state.lock().unwrap();
            let now = state.now;
            let document = state.documents.get_mut(key).unwrap();
            if document.read_failed
                || document.active.is_some()
                || document.retry_at > now
                || (document.value == document.persisted && document.error.is_none())
            {
                return;
            }
            document.active = Some(document.revision);
            (document.revision, document.value.clone())
        };
        let owner = self.clone();
        let key_owned = key.to_owned();
        let written = value.clone();
        self.backend.write(
            key,
            value,
            Box::new(move |result| {
                let success = result.is_ok();
                {
                    let mut state = owner.state.lock().unwrap();
                    let now = state.now;
                    let document = state.documents.get_mut(&key_owned).unwrap();
                    assert_eq!(document.active, Some(revision));
                    document.active = None;
                    match result {
                        Ok(()) => {
                            document.persisted = written;
                            document.error = None;
                            document.retry_at = 0;
                        }
                        Err(error) => {
                            document.error = Some(error.chars().take(512).collect());
                            document.retry_at = now.saturating_add(RETRY_MS);
                        }
                    }
                }
                if success {
                    owner.pump(&key_owned);
                }
            }),
        );
    }

    pub fn tick(self: &Arc<Self>, now: i64) {
        let keys = {
            let mut state = self.state.lock().unwrap();
            state.now = now;
            if state.refresh_at.is_some_and(|deadline| deadline <= now) {
                state.refresh_at = None;
            }
            state.documents.keys().cloned().collect::<Vec<_>>()
        };
        for key in keys {
            self.pump(&key);
        }
    }

    pub fn errors(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .documents
            .iter()
            .filter_map(|(key, doc)| doc.error.as_ref().map(|error| format!("{key}: {error}")))
            .collect()
    }

    pub fn next_refresh(&self, now: i64) -> Option<i64> {
        let state = self.state.lock().unwrap();
        state
            .documents
            .values()
            .filter_map(|doc| {
                if doc.active.is_some() {
                    Some(now.saturating_add(1_000))
                } else if doc.error.is_some() && !doc.read_failed {
                    Some(doc.retry_at.max(now.saturating_add(1)))
                } else {
                    None
                }
            })
            .chain(state.refresh_at)
            .min()
    }

    /// Mutations stage writes after projection. Advertise their completion check
    /// in that projection rather than waiting for unrelated input.
    pub fn arm_completion_check(&self, now: i64) {
        self.state.lock().unwrap().refresh_at = Some(now.saturating_add(1_000));
    }
}

fn storage_error(message: &str) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Write = (String, Option<Vec<u8>>, DocumentCompletion);
    #[derive(Default)]
    struct Host {
        files: Mutex<BTreeMap<String, Vec<u8>>>,
        writes: Mutex<Vec<Write>>,
    }
    impl LocalDocumentBackend for Host {
        fn read(&self, key: &str) -> AppResult<Option<Vec<u8>>> {
            Ok(self.files.lock().unwrap().get(key).cloned())
        }
        fn write(&self, key: &str, bytes: Option<Vec<u8>>, complete: DocumentCompletion) {
            self.writes
                .lock()
                .unwrap()
                .push((key.into(), bytes, complete));
        }
    }
    impl Host {
        fn finish(&self, index: usize, result: Result<(), String>) {
            let (key, value, complete) = self.writes.lock().unwrap().remove(index);
            if result.is_ok() {
                let mut files = self.files.lock().unwrap();
                if let Some(bytes) = value {
                    files.insert(key, bytes);
                } else {
                    files.remove(&key);
                }
            }
            complete(result);
        }
    }
    #[test]
    fn serializes_each_document_and_coalesces_pending_versions() {
        let host = Arc::new(Host::default());
        let store = LocalDocuments::new(host.clone());
        store.replace("a", b"first").unwrap();
        store.replace("a", b"second").unwrap();
        store.replace("a", b"latest").unwrap();
        store.replace("b", b"independent").unwrap();
        assert_eq!(host.writes.lock().unwrap().len(), 2);
        host.finish(1, Ok(())); // Other documents may finish out of order.
        host.finish(0, Ok(()));
        assert_eq!(
            host.writes.lock().unwrap()[0].1.as_deref(),
            Some(b"latest".as_slice())
        );
        host.finish(0, Ok(()));
        let restarted = LocalDocuments::new(host.clone());
        assert_eq!(
            restarted.read("a").unwrap().as_deref(),
            Some(b"latest".as_slice())
        );
        store.replace("a", b"latest").unwrap();
        assert!(host.writes.lock().unwrap().is_empty());
    }
    #[test]
    fn failed_write_retains_disk_and_local_edit_then_retries_latest() {
        let host = Arc::new(Host::default());
        host.files
            .lock()
            .unwrap()
            .insert("a".into(), b"old".to_vec());
        let store = LocalDocuments::new(host.clone());
        store.replace("a", b"edit").unwrap();
        host.finish(0, Err("disk full".into()));
        assert_eq!(store.errors(), ["a: disk full"]);
        assert_eq!(
            store.read("a").unwrap().as_deref(),
            Some(b"edit".as_slice())
        );
        assert_eq!(host.read("a").unwrap().as_deref(), Some(b"old".as_slice()));
        store.replace("a", b"newer").unwrap();
        store.tick(RETRY_MS - 1);
        assert!(host.writes.lock().unwrap().is_empty());
        store.tick(RETRY_MS);
        host.finish(0, Ok(()));
        assert!(store.errors().is_empty());
        assert_eq!(
            host.read("a").unwrap().as_deref(),
            Some(b"newer".as_slice())
        );
        store.delete("a").unwrap();
        host.finish(0, Ok(()));
        assert_eq!(LocalDocuments::new(host).read("a").unwrap(), None);
    }
    #[test]
    fn reverting_while_write_is_in_flight_still_restores_the_old_value() {
        let host = Arc::new(Host::default());
        host.files
            .lock()
            .unwrap()
            .insert("a".into(), b"old".to_vec());
        let store = LocalDocuments::new(host.clone());
        store.replace("a", b"new").unwrap();
        store.replace("a", b"old").unwrap();
        host.finish(0, Ok(()));
        assert_eq!(host.writes.lock().unwrap().len(), 1);
        host.finish(0, Ok(()));
        assert_eq!(host.read("a").unwrap().as_deref(), Some(b"old".as_slice()));
    }

    #[test]
    fn revert_after_failure_retries_even_if_disk_outcome_was_uncertain() {
        let host = Arc::new(Host::default());
        host.files
            .lock()
            .unwrap()
            .insert("a".into(), b"old".to_vec());
        let store = LocalDocuments::new(host.clone());
        store.replace("a", b"new").unwrap();
        // A host may replace the file and then fail its durability barrier.
        host.files
            .lock()
            .unwrap()
            .insert("a".into(), b"new".to_vec());
        host.finish(0, Err("sync failed".into()));
        store.replace("a", b"old").unwrap();
        store.tick(RETRY_MS);
        assert_eq!(host.writes.lock().unwrap().len(), 1);
        host.finish(0, Ok(()));
        assert_eq!(host.read("a").unwrap().as_deref(), Some(b"old".as_slice()));
        assert!(store.errors().is_empty());
        assert_eq!(store.next_refresh(RETRY_MS), None);
    }

    #[test]
    fn failed_reads_are_not_absence_and_cannot_be_overwritten() {
        struct Denied;
        impl LocalDocumentBackend for Denied {
            fn read(&self, _: &str) -> AppResult<Option<Vec<u8>>> {
                Err(storage_error("read denied"))
            }
            fn write(&self, _: &str, _: Option<Vec<u8>>, _: DocumentCompletion) {
                panic!("must not replace unread data")
            }
        }
        let store = LocalDocuments::new(Arc::new(Denied));
        assert!(store.read("a").is_err());
        assert!(store.replace("a", b"new").is_err());
        assert!(store.delete("a").is_err());
        store.tick(90_000);
        assert_eq!(store.errors(), ["a: read denied"]);
    }
}

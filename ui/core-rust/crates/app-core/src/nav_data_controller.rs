// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use app_ui_contracts::session::UiNavDbIdentity;

use crate::{
    freshness::parse_utc_instant, package_controller::PackageController, CoreResourcePolicy,
    CoreResourceRequest, NavDbArtifactCandidate, NavDbOpenResult, NavKvStore,
};

pub(crate) const NAV_DB_PUBLICATION_POLL_INTERVAL_MS: i64 = 4 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachedNavDbArtifact {
    pub package_id: String,
    pub filename: String,
    pub contract_id: Option<String>,
    pub cycle: Option<String>,
    pub cycle_version: Option<String>,
    pub effective_date: Option<String>,
    pub expiration_date: Option<String>,
    pub warning_text: Option<String>,
}

impl From<&NavDbOpenResult> for AttachedNavDbArtifact {
    fn from(result: &NavDbOpenResult) -> Self {
        Self {
            package_id: result.selected_package_id.clone(),
            filename: result.selected_filename.clone(),
            contract_id: result.selected_contract_id.clone(),
            cycle: result.selected_cycle.clone(),
            cycle_version: result.selected_cycle_version.clone(),
            effective_date: result.selected_effective_date.clone(),
            expiration_date: result.selected_expiration_date.clone(),
            warning_text: result.selected_warning_text.clone(),
        }
    }
}

impl From<&AttachedNavDbArtifact> for UiNavDbIdentity {
    fn from(artifact: &AttachedNavDbArtifact) -> Self {
        Self {
            package_id: artifact.package_id.clone(),
            filename: artifact.filename.clone(),
            contract_id: artifact.contract_id.clone(),
            cycle: artifact.cycle.clone(),
            cycle_version: artifact.cycle_version.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NavDataModel {
    epoch: u64,
    advance_blocked: bool,
    active_artifact: Option<AttachedNavDbArtifact>,
    revision: u64,
}

#[derive(Default)]
struct NavDataRuntime {
    store_id: Option<u32>,
    store: Option<Arc<NavKvStore>>,
    generation: u64,
}

#[derive(Clone)]
pub(crate) struct NavDataModelCheckpoint {
    model: NavDataModel,
}

pub(crate) enum NavDataMaintenanceDecision {
    None,
    AttemptAdvance,
    NeedResources(Vec<CoreResourceRequest>),
}

#[derive(Default)]
pub(crate) struct NavDataController {
    model: NavDataModel,
    runtime: NavDataRuntime,
}

impl NavDataController {
    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn epoch(&self) -> u64 {
        self.model.epoch
    }

    pub fn generation(&self) -> u64 {
        self.runtime.generation
    }

    pub fn advance_blocked(&self) -> bool {
        self.model.advance_blocked
    }

    pub fn active_artifact(&self) -> Option<&AttachedNavDbArtifact> {
        self.model.active_artifact.as_ref()
    }

    pub fn active_identity(&self) -> Option<UiNavDbIdentity> {
        self.model
            .active_artifact
            .as_ref()
            .map(UiNavDbIdentity::from)
    }

    #[cfg(test)]
    pub fn store_id(&self) -> Option<u32> {
        self.runtime.store_id
    }

    pub fn store(&self) -> Option<&NavKvStore> {
        self.runtime.store.as_deref()
    }

    pub fn store_arc(&self) -> Option<Arc<NavKvStore>> {
        self.runtime.store.clone()
    }

    pub fn checkpoint_model(&self) -> NavDataModelCheckpoint {
        NavDataModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: NavDataModelCheckpoint) {
        self.model = checkpoint.model;
    }

    pub fn attach(
        &mut self,
        store_id: u32,
        store: &NavKvStore,
        open_result: Option<&NavDbOpenResult>,
    ) {
        self.runtime.store_id = Some(store_id);
        self.runtime.store = Some(Arc::new(store.clone()));
        self.runtime.generation = self.runtime.generation.saturating_add(1);
        self.model.active_artifact = open_result.map(AttachedNavDbArtifact::from);
        self.note_model_change();
    }

    pub fn candidate(
        &self,
        store_id: u32,
        store: &NavKvStore,
        open_result: &NavDbOpenResult,
    ) -> Self {
        let artifact = AttachedNavDbArtifact::from(open_result);
        let changed = self
            .model
            .active_artifact
            .as_ref()
            .is_none_or(|active| active.filename != artifact.filename);
        Self {
            model: NavDataModel {
                epoch: self.model.epoch.saturating_add(u64::from(changed)),
                advance_blocked: false,
                active_artifact: Some(artifact),
                revision: self.model.revision.saturating_add(1),
            },
            runtime: NavDataRuntime {
                store_id: Some(store_id),
                store: Some(Arc::new(store.clone())),
                generation: self.runtime.generation.saturating_add(1),
            },
        }
    }

    pub fn block_advance(&mut self) {
        if !self.model.advance_blocked {
            self.model.advance_blocked = true;
            self.note_model_change();
        }
    }

    pub fn insert_page_if_attached(
        &mut self,
        store_id: u32,
        page_index: u32,
        page_bytes: &[u8],
    ) -> bool {
        if self.runtime.store_id != Some(store_id) {
            return false;
        }
        if let Some(store) = self.runtime.store.as_mut() {
            Arc::make_mut(store).insert_page(page_index, page_bytes.to_vec());
        }
        true
    }

    pub fn clear_pages_if_attached(&mut self, store_id: u32) -> bool {
        if self.runtime.store_id != Some(store_id) {
            return false;
        }
        if let Some(store) = self.runtime.store.as_mut() {
            Arc::make_mut(store).clear_pages();
        }
        true
    }

    pub fn next_maintenance_epoch_ms(
        &self,
        packages: &PackageController,
        now_epoch_ms: i64,
    ) -> Option<i64> {
        if self.model.advance_blocked {
            return None;
        }
        let resource_policy = packages.resource_policy();
        let mut next = if resource_policy == CoreResourcePolicy::PublicUnpacked {
            Some(
                packages
                    .current_artifacts_checked_epoch_ms()
                    .map(|checked| checked.saturating_add(NAV_DB_PUBLICATION_POLL_INTERVAL_MS))
                    .unwrap_or(now_epoch_ms),
            )
        } else {
            None
        };
        let candidates = match packages.nav_db_artifact_candidates() {
            Ok(Ok(candidates)) => candidates,
            Ok(Err(_)) | Err(_) => {
                return if resource_policy == CoreResourcePolicy::PublicUnpacked {
                    min_optional_epoch_ms(next, Some(now_epoch_ms))
                } else {
                    None
                };
            }
        };
        let preferred =
            crate::had_ops::select_preferred_nav_db_candidate(&candidates, now_epoch_ms);
        if preferred.is_some_and(|candidate| self.candidate_differs(candidate)) {
            next = min_optional_epoch_ms(next, Some(now_epoch_ms));
        }
        let next_effective = candidates
            .iter()
            .filter_map(candidate_effective_epoch_ms)
            .filter(|effective| *effective > now_epoch_ms)
            .min();
        min_optional_epoch_ms(next, next_effective)
    }

    pub fn maintenance_decision(
        &self,
        packages: &PackageController,
        now_epoch_ms: i64,
    ) -> Result<NavDataMaintenanceDecision, String> {
        if self.model.advance_blocked {
            return Ok(NavDataMaintenanceDecision::None);
        }
        let resource_policy = packages.resource_policy();
        if resource_policy == CoreResourcePolicy::PublicUnpacked {
            let refresh_due = packages
                .current_artifacts_checked_epoch_ms()
                .is_none_or(|checked| {
                    checked.saturating_add(NAV_DB_PUBLICATION_POLL_INTERVAL_MS) <= now_epoch_ms
                });
            if refresh_due {
                return packages
                    .current_artifacts_refresh_request()
                    .map(|resource| NavDataMaintenanceDecision::NeedResources(vec![resource]));
            }
        }
        let candidates = match packages.nav_db_artifact_candidates()? {
            Ok(candidates) => candidates,
            Err(resources) if resource_policy == CoreResourcePolicy::PublicUnpacked => {
                return Ok(NavDataMaintenanceDecision::NeedResources(resources));
            }
            Err(_) => return Ok(NavDataMaintenanceDecision::None),
        };
        let preferred =
            crate::had_ops::select_preferred_nav_db_candidate(&candidates, now_epoch_ms);
        Ok(
            if preferred.is_some_and(|candidate| self.candidate_differs(candidate)) {
                NavDataMaintenanceDecision::AttemptAdvance
            } else {
                NavDataMaintenanceDecision::None
            },
        )
    }

    fn candidate_differs(&self, candidate: &NavDbArtifactCandidate) -> bool {
        self.model
            .active_artifact
            .as_ref()
            .is_none_or(|active| active.filename != candidate.filename)
    }

    fn note_model_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
    }
}

fn candidate_effective_epoch_ms(candidate: &NavDbArtifactCandidate) -> Option<i64> {
    candidate
        .effective_date
        .as_deref()
        .and_then(parse_utc_instant)
        .map(|instant| instant.timestamp_millis())
}

fn min_optional_epoch_ms(current: Option<i64>, candidate: Option<i64>) -> Option<i64> {
    match (current, candidate) {
        (Some(current), Some(candidate)) => Some(current.min(candidate)),
        (Some(current), None) => Some(current),
        (None, Some(candidate)) => Some(candidate),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_result(package_id: &str) -> NavDbOpenResult {
        NavDbOpenResult {
            selected_package_id: package_id.to_string(),
            selected_filename: format!("{package_id}.zip"),
            selected_contract_id: None,
            selected_cycle: None,
            selected_cycle_version: None,
            selected_effective_date: None,
            selected_expiration_date: None,
            selected_warning_text: None,
            statuses: Vec::new(),
        }
    }

    #[test]
    fn candidate_is_invisible_until_the_controller_is_swapped() {
        let old_store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let new_store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut active = NavDataController::default();
        active.attach(1, &old_store, Some(&open_result("NAV_DB_2607")));
        let candidate = active.candidate(2, &new_store, &open_result("NAV_DB_2608"));

        assert_eq!(active.epoch(), 0);
        assert_eq!(active.store_id(), Some(1));
        assert_eq!(
            active
                .active_artifact()
                .map(|artifact| artifact.package_id.as_str()),
            Some("NAV_DB_2607")
        );
        assert_eq!(candidate.epoch(), 1);
        assert_eq!(candidate.store_id(), Some(2));
    }

    #[test]
    fn replacing_the_same_artifact_advances_generation_but_not_public_epoch() {
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut active = NavDataController::default();
        active.attach(1, &store, Some(&open_result("NAV_DB_2607")));

        let candidate = active.candidate(2, &store, &open_result("NAV_DB_2607"));

        assert_eq!(candidate.epoch(), active.epoch());
        assert_eq!(candidate.generation(), active.generation() + 1);
        assert_eq!(candidate.revision(), active.revision() + 1);
    }

    #[test]
    fn model_checkpoint_restores_identity_epoch_and_blocking() {
        let store = crate::navkv::nav_kv_store_for_test(&[], 256);
        let mut controller = NavDataController::default();
        controller.attach(1, &store, Some(&open_result("NAV_DB_2607")));
        let checkpoint = controller.checkpoint_model();
        let mut candidate = controller.candidate(2, &store, &open_result("NAV_DB_2608"));
        candidate.block_advance();

        candidate.rollback_model(checkpoint);
        assert_eq!(candidate.epoch(), 0);
        assert!(!candidate.advance_blocked());
        assert_eq!(
            candidate
                .active_artifact()
                .map(|artifact| artifact.package_id.as_str()),
            Some("NAV_DB_2607")
        );
    }
}

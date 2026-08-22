// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{
    cloud::{CloudCompletion, CloudEngine},
    AppResult,
};
use crate::{
    CloudEventStreamEvent, CloudEventStreamPlan, CloudHttpRequest, CloudHttpResponse,
    CloudPersistentState, CloudStatusSummary, CloudUiActionId, CloudUiFieldValue, DataStatusRecord,
    DebugFlagId, FlightPlan, InactivitySleepTimeout, OfflinePackagePreferences, UiCloudPageState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloudProjectionInput {
    pub now_epoch_ms: i64,
    pub qr_scanner_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CloudProjection {
    pub page_state: UiCloudPageState,
    pub status_summary: CloudStatusSummary,
    pub status_record: Option<DataStatusRecord>,
}

pub(crate) struct CloudProjectionResult {
    pub projection: CloudProjection,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct CloudProjectionCache {
    revision: u64,
    input: CloudProjectionInput,
    projection: CloudProjection,
}

#[derive(Clone)]
struct CloudModel {
    engine: Arc<CloudEngine>,
    revision: u64,
}

#[derive(Clone)]
pub(crate) struct CloudModelCheckpoint {
    model: CloudModel,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CloudDomainUpdates {
    pub remote_flight_plan: Option<FlightPlan>,
    pub offline_package_preferences: Option<OfflinePackagePreferences>,
    pub inactivity_sleep_timeout: Option<InactivitySleepTimeout>,
    pub nexrad_acquisition: Option<crate::NexradAcquisitionPreferences>,
    pub debug_flags: Vec<(DebugFlagId, bool)>,
    pub aircraft_library_changed: bool,
}

pub(crate) struct CloudController {
    model: CloudModel,
    projection_cache: Option<CloudProjectionCache>,
    projection_revision: u64,
}

impl Default for CloudController {
    fn default() -> Self {
        Self::new(CloudPersistentState::default())
    }
}

impl CloudController {
    pub fn new(persistent: CloudPersistentState) -> Self {
        Self {
            model: CloudModel {
                engine: Arc::new(CloudEngine::new(persistent)),
                revision: 0,
            },
            projection_cache: None,
            projection_revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn projection_revision(&self) -> u64 {
        self.projection_revision
    }

    pub fn checkpoint_model(&self) -> CloudModelCheckpoint {
        CloudModelCheckpoint {
            model: self.model.clone(),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: CloudModelCheckpoint) {
        self.model = checkpoint.model;
    }

    pub fn persistent(&self) -> &CloudPersistentState {
        self.engine().persistent()
    }

    pub fn cached_flight_plan(&self) -> Option<FlightPlan> {
        self.engine().cached_flight_plan()
    }

    pub fn offline_package_preferences(&self) -> AppResult<OfflinePackagePreferences> {
        self.engine().offline_package_preferences()
    }

    pub fn inactivity_sleep_timeout(&self) -> AppResult<Option<InactivitySleepTimeout>> {
        self.engine().inactivity_sleep_timeout()
    }

    pub fn nexrad_acquisition(&self) -> AppResult<Option<crate::NexradAcquisitionPreferences>> {
        self.engine().nexrad_acquisition()
    }

    pub fn debug_flags(&self) -> AppResult<Vec<(DebugFlagId, bool)>> {
        self.engine().debug_flags()
    }

    pub fn aircraft_definitions(
        &self,
    ) -> AppResult<BTreeMap<String, product_contracts::AircraftDefinition>> {
        self.engine().aircraft_definitions()
    }

    pub fn aircraft_library_digest(&self) -> AppResult<[u8; 32]> {
        self.engine().aircraft_library_digest()
    }

    pub fn record_local_aircraft_definition(
        &mut self,
        definition: &product_contracts::AircraftDefinition,
    ) -> AppResult<bool> {
        let changed = self
            .engine_mut()
            .record_local_aircraft_definition(definition)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn aircraft_library_memberships(
        &self,
    ) -> AppResult<BTreeMap<String, product_contracts::AircraftLibraryMembership>> {
        self.engine().aircraft_library_memberships()
    }

    pub fn record_local_aircraft_library_membership(
        &mut self,
        definition_hash: &str,
        membership: product_contracts::AircraftLibraryMembership,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed = self.engine_mut().record_local_aircraft_library_membership(
            definition_hash,
            membership,
            now_epoch_ms,
        )?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn event_stream_plan(&self) -> Option<CloudEventStreamPlan> {
        self.engine().event_stream_plan()
    }

    pub fn set_acs_default_base_url(&mut self, base_url: Option<String>) -> AppResult<()> {
        self.engine_mut().set_acs_default_base_url(base_url)?;
        self.note_change();
        Ok(())
    }

    pub fn report_event_stream_event(
        &mut self,
        event: CloudEventStreamEvent,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        self.engine_mut()
            .report_event_stream_event(event, now_epoch_ms)?;
        self.note_change();
        Ok(())
    }

    pub fn perform_ui_action(
        &mut self,
        action_id: CloudUiActionId,
        fields: &[CloudUiFieldValue],
        current_plan: &FlightPlan,
        now_epoch_ms: i64,
    ) -> AppResult<()> {
        self.engine_mut()
            .perform_ui_action(action_id, fields, current_plan, now_epoch_ms)?;
        self.note_change();
        Ok(())
    }

    pub fn record_local_offline_package_preferences(
        &mut self,
        preferences: &OfflinePackagePreferences,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed = self
            .engine_mut()
            .record_local_offline_package_preferences(preferences, now_epoch_ms)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn record_local_inactivity_sleep_timeout(
        &mut self,
        timeout: InactivitySleepTimeout,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed = self
            .engine_mut()
            .record_local_inactivity_sleep_timeout(timeout, now_epoch_ms)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn record_local_nexrad_acquisition(
        &mut self,
        preferences: crate::NexradAcquisitionPreferences,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed = self
            .engine_mut()
            .record_local_nexrad_acquisition(preferences, now_epoch_ms)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn record_local_debug_flag(
        &mut self,
        flag_id: DebugFlagId,
        enabled: bool,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed = self
            .engine_mut()
            .record_local_debug_flag(flag_id, enabled, now_epoch_ms)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn take_provider_request(
        &mut self,
        now_epoch_ms: i64,
    ) -> AppResult<Option<CloudHttpRequest>> {
        let request = self.engine_mut().take_provider_request(now_epoch_ms)?;
        if request.is_some() {
            self.note_change();
        }
        Ok(request)
    }

    pub fn complete_provider_request(
        &mut self,
        request_id: u64,
        response: CloudHttpResponse,
        now_epoch_ms: i64,
    ) -> AppResult<CloudDomainUpdates> {
        let completion =
            self.engine_mut()
                .complete_provider_request(request_id, response, now_epoch_ms)?;
        self.note_change();
        self.domain_updates(&completion)
    }

    pub fn record_local_flight_plan_mutation(
        &mut self,
        before: &FlightPlan,
        after: &FlightPlan,
        now_epoch_ms: i64,
    ) -> AppResult<bool> {
        let changed =
            self.engine_mut()
                .record_local_flight_plan_mutation(before, after, now_epoch_ms)?;
        if changed {
            self.note_change();
        }
        Ok(changed)
    }

    pub fn take_pending_remote_flight_plan(&mut self) -> Option<FlightPlan> {
        let plan = self.engine_mut().take_pending_remote_flight_plan();
        if plan.is_some() {
            self.note_change();
        }
        plan
    }

    pub fn set_pending_remote_flight_plan(&mut self, plan: FlightPlan) -> AppResult<()> {
        self.engine_mut().set_pending_remote_flight_plan(plan)?;
        self.note_change();
        Ok(())
    }

    pub fn project(&mut self, input: CloudProjectionInput) -> CloudProjectionResult {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.revision == self.model.revision && cache.input == input {
                return CloudProjectionResult {
                    projection: cache.projection.clone(),
                    rebuilt: false,
                };
            }
        }
        let projection = CloudProjection {
            page_state: self
                .engine()
                .page_state_with_qr_scanner(input.now_epoch_ms, input.qr_scanner_available),
            status_summary: self.engine().status_summary(input.now_epoch_ms),
            status_record: self.engine().status_record(input.now_epoch_ms),
        };
        if self
            .projection_cache
            .as_ref()
            .is_none_or(|cache| cache.projection != projection)
        {
            self.projection_revision = self.projection_revision.saturating_add(1);
        }
        self.projection_cache = Some(CloudProjectionCache {
            revision: self.model.revision,
            input,
            projection: projection.clone(),
        });
        CloudProjectionResult {
            projection,
            rebuilt: true,
        }
    }

    fn domain_updates(&self, completion: &CloudCompletion) -> AppResult<CloudDomainUpdates> {
        Ok(CloudDomainUpdates {
            remote_flight_plan: completion.remote_flight_plan()?,
            offline_package_preferences: completion
                .offline_package_preferences_changed()
                .then(|| self.offline_package_preferences())
                .transpose()?,
            inactivity_sleep_timeout: completion
                .inactivity_sleep_timeout_changed()
                .then(|| self.inactivity_sleep_timeout())
                .transpose()?
                .flatten(),
            nexrad_acquisition: completion
                .nexrad_acquisition_changed()
                .then(|| self.nexrad_acquisition())
                .transpose()?
                .flatten(),
            debug_flags: completion.debug_flags()?,
            aircraft_library_changed: completion.aircraft_library_changed(),
        })
    }

    fn engine(&self) -> &CloudEngine {
        &self.model.engine
    }

    fn engine_mut(&mut self) -> &mut CloudEngine {
        Arc::make_mut(&mut self.model.engine)
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projection_is_cached_by_revision_clock_and_capability() {
        let mut controller = CloudController::default();
        let input = CloudProjectionInput {
            now_epoch_ms: 100,
            qr_scanner_available: false,
        };
        assert!(controller.project(input).rebuilt);
        let initial_projection_revision = controller.projection_revision();
        assert!(!controller.project(input).rebuilt);
        assert!(
            controller
                .project(CloudProjectionInput {
                    now_epoch_ms: 101,
                    ..input
                })
                .rebuilt
        );
        assert_eq!(
            controller.projection_revision(),
            initial_projection_revision
        );
        assert!(
            controller
                .project(CloudProjectionInput {
                    qr_scanner_available: true,
                    ..input
                })
                .rebuilt
        );
    }

    #[test]
    fn checkpoint_is_copy_on_write_and_rollback_restores_engine() {
        let mut controller = CloudController::default();
        let engine_address = Arc::as_ptr(&controller.model.engine);
        let checkpoint = controller.checkpoint_model();
        assert_eq!(Arc::as_ptr(&controller.model.engine), engine_address);

        controller
            .set_acs_default_base_url(Some("https://cloud.example/cloud/".to_string()))
            .unwrap();
        assert_ne!(Arc::as_ptr(&controller.model.engine), engine_address);

        controller.rollback_model(checkpoint);
        assert_eq!(Arc::as_ptr(&controller.model.engine), engine_address);
        assert_eq!(controller.revision(), 0);
    }
}

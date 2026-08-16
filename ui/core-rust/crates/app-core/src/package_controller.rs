// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    publication::PublicationResolver, AppError, AppErrorKind, AppResult, BundlePackageArtifact,
    CoreResourcePolicy, CoreResourceRequest, HadOperationOutcome, NavDbArtifactCandidate,
    OfflinePackagePreferences, OfflinePackagesLibraryCache,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PackageModel {
    installed_package_ids: BTreeSet<String>,
    preferences: OfflinePackagePreferences,
    revision: u64,
}

#[derive(Clone)]
struct PackageProjectionCache {
    revision: u64,
    offline_package_preferences_json: String,
}

#[derive(Clone)]
pub(crate) struct PackageModelCheckpoint {
    model: PackageModel,
    resolver: Arc<PublicationResolver>,
}

pub(crate) struct PackageProjectionResult {
    pub offline_package_preferences_json: String,
    pub rebuilt: bool,
}

pub(crate) struct PackageController {
    model: PackageModel,
    resolver: Arc<PublicationResolver>,
    projection_cache: Option<PackageProjectionCache>,
}

impl Default for PackageController {
    fn default() -> Self {
        Self::new("/packages", CoreResourcePolicy::InstalledPackage)
    }
}

impl PackageController {
    pub fn new(public_base_url: impl Into<String>, resource_policy: CoreResourcePolicy) -> Self {
        Self {
            model: PackageModel::default(),
            resolver: Arc::new(PublicationResolver::with_resource_policy(
                public_base_url,
                resource_policy,
            )),
            projection_cache: None,
        }
    }

    pub fn revision(&self) -> u64 {
        self.model.revision
    }

    pub fn resource_policy(&self) -> CoreResourcePolicy {
        self.resolver.resource_policy()
    }

    pub fn installed_package_ids(&self) -> &BTreeSet<String> {
        &self.model.installed_package_ids
    }

    #[cfg(test)]
    pub fn preferences(&self) -> &OfflinePackagePreferences {
        &self.model.preferences
    }

    #[cfg(test)]
    pub fn resolver(&self) -> &PublicationResolver {
        &self.resolver
    }

    pub fn checkpoint_model(&self) -> PackageModelCheckpoint {
        PackageModelCheckpoint {
            model: self.model.clone(),
            resolver: Arc::clone(&self.resolver),
        }
    }

    pub fn rollback_model(&mut self, checkpoint: PackageModelCheckpoint) {
        self.model = checkpoint.model;
        self.resolver = checkpoint.resolver;
        self.projection_cache = None;
    }

    pub fn set_resource_policy(&mut self, resource_policy: CoreResourcePolicy) -> bool {
        if self.resolver.resource_policy() == resource_policy {
            return false;
        }
        Arc::make_mut(&mut self.resolver).set_resource_policy(resource_policy);
        self.note_change();
        true
    }

    pub fn set_installed_package_ids(
        &mut self,
        installed_package_ids: impl IntoIterator<Item = String>,
    ) -> bool {
        let installed_package_ids = installed_package_ids.into_iter().collect();
        if self.model.installed_package_ids == installed_package_ids {
            return false;
        }
        self.model.installed_package_ids = installed_package_ids;
        self.note_change();
        true
    }

    pub fn insert_installed_package_id(&mut self, package_id: String) -> bool {
        let changed = self.model.installed_package_ids.insert(package_id);
        if changed {
            self.note_change();
        }
        changed
    }

    pub fn replace_preferences(&mut self, preferences: OfflinePackagePreferences) -> bool {
        if self.model.preferences == preferences {
            return false;
        }
        self.model.preferences = preferences;
        self.note_change();
        true
    }

    pub fn load_offline_library_cache(
        &mut self,
        cache: &OfflinePackagesLibraryCache,
    ) -> Result<(), String> {
        Arc::make_mut(&mut self.resolver).load_offline_library_cache(cache)?;
        self.note_change();
        Ok(())
    }

    pub fn ingest_resource(
        &mut self,
        resource_id: &str,
        resource_bytes: &[u8],
    ) -> Result<(), String> {
        Arc::make_mut(&mut self.resolver).ingest_resource(resource_id, resource_bytes)?;
        self.note_change();
        Ok(())
    }

    pub fn ingest_resource_at_epoch_ms(
        &mut self,
        resource_id: &str,
        resource_bytes: &[u8],
        epoch_ms: i64,
    ) -> Result<(), String> {
        Arc::make_mut(&mut self.resolver).ingest_resource_at_epoch_ms(
            resource_id,
            resource_bytes,
            epoch_ms,
        )?;
        self.note_change();
        Ok(())
    }

    pub fn resolve_nav_db_artifact_candidates(&self) -> Result<HadOperationOutcome, String> {
        self.resolver.resolve_nav_db_artifact_candidates()
    }

    pub fn nav_db_artifact_candidates(
        &self,
    ) -> Result<Result<Vec<NavDbArtifactCandidate>, Vec<CoreResourceRequest>>, String> {
        let candidates = match self.resolver.nav_db_artifact_candidates()? {
            Ok(candidates) => candidates,
            Err(resources) => return Ok(Err(resources)),
        };
        if self.resource_policy() == CoreResourcePolicy::PublicUnpacked {
            return Ok(Ok(candidates));
        }
        Ok(Ok(candidates
            .into_iter()
            .filter(|candidate| {
                self.model
                    .installed_package_ids
                    .contains(&candidate.package_id)
            })
            .collect()))
    }

    pub fn resolve_family_resource(
        &self,
        family_id: &str,
        member_path: &str,
    ) -> Result<HadOperationOutcome, String> {
        self.resolver
            .resolve_family_resource(family_id, member_path)
    }

    pub fn package_resource_requests(
        &self,
        target_resource_id: &str,
        package_id: &str,
        member_path: &str,
        optional: bool,
    ) -> Result<Vec<CoreResourceRequest>, String> {
        self.resolver.package_resource_requests(
            target_resource_id,
            package_id,
            member_path,
            optional,
        )
    }

    pub fn package_member_public_url(
        &self,
        package_id: &str,
        member_path: &str,
    ) -> Result<String, String> {
        self.resolver
            .package_member_public_url(package_id, member_path)
    }

    pub fn current_artifacts(&self) -> Option<&crate::CurrentArtifactsManifest> {
        self.resolver.current_artifacts()
    }

    pub fn current_artifacts_checked_epoch_ms(&self) -> Option<i64> {
        self.resolver.current_artifacts_checked_epoch_ms()
    }

    pub fn current_artifacts_refresh_request(&self) -> Result<CoreResourceRequest, String> {
        self.resolver.current_artifacts_refresh_request()
    }

    pub fn loaded_bundle_manifest_count(&self) -> usize {
        self.resolver.loaded_bundle_manifest_count()
    }

    pub fn loaded_bundle_packages(&self) -> impl Iterator<Item = &BundlePackageArtifact> {
        self.resolver.loaded_bundle_packages()
    }

    pub fn project(&mut self) -> AppResult<PackageProjectionResult> {
        if let Some(cache) = self.projection_cache.as_ref() {
            if cache.revision == self.model.revision {
                return Ok(PackageProjectionResult {
                    offline_package_preferences_json: cache
                        .offline_package_preferences_json
                        .clone(),
                    rebuilt: false,
                });
            }
        }
        let offline_package_preferences_json = serde_json::to_string(&self.model.preferences)
            .map_err(|error| AppError {
                kind: AppErrorKind::Internal,
                message: format!("offline-package preferences serialization failed: {error}"),
            })?;
        self.projection_cache = Some(PackageProjectionCache {
            revision: self.model.revision,
            offline_package_preferences_json: offline_package_preferences_json.clone(),
        });
        Ok(PackageProjectionResult {
            offline_package_preferences_json,
            rebuilt: true,
        })
    }

    fn note_change(&mut self) {
        self.model.revision = self.model.revision.saturating_add(1);
        self.projection_cache = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_cached_and_invalidated_by_preferences() {
        let mut controller = PackageController::default();
        assert!(controller.project().expect("first projection").rebuilt);
        assert!(!controller.project().expect("cached projection").rebuilt);

        let mut preferences = OfflinePackagePreferences::default();
        preferences
            .products
            .insert("terrain".to_string(), crate::OfflinePackageSelection::Pause);
        assert!(controller.replace_preferences(preferences));
        assert!(controller.project().expect("changed projection").rebuilt);
    }

    #[test]
    fn checkpoint_restores_model_and_resolver_policy() {
        let mut controller = PackageController::default();
        let checkpoint = controller.checkpoint_model();
        controller.set_resource_policy(CoreResourcePolicy::PublicUnpacked);
        controller.set_installed_package_ids(["nav-db".to_string()]);

        controller.rollback_model(checkpoint);

        assert_eq!(
            controller.resource_policy(),
            CoreResourcePolicy::InstalledPackage
        );
        assert_eq!(
            controller.resolver().resource_policy(),
            CoreResourcePolicy::InstalledPackage
        );
        assert!(controller.installed_package_ids().is_empty());
    }
}

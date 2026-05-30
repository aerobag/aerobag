use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    BundleManifest, BundlePackageArtifact, CoreResourceRequest, CoreResourceSource,
    CurrentArtifactsManifest, HadOperationOutcome, NavDbArtifactCandidate,
    REQUIRED_NAV_DB_CONTRACT_VERSION,
};

const CURRENT_ARTIFACTS_RESOURCE_ID: &str = "publication/current_artifacts";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreResourcePolicy {
    PublicUnpacked,
    InstalledPackage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationResolver {
    public_base_url: String,
    resource_policy: CoreResourcePolicy,
    current_artifacts: Option<CurrentArtifactsManifest>,
    bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationResolvedResource {
    pub source: CoreResourceSource,
}

impl PublicationResolver {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self::with_resource_policy(public_base_url, CoreResourcePolicy::PublicUnpacked)
    }

    pub fn with_resource_policy(
        public_base_url: impl Into<String>,
        resource_policy: CoreResourcePolicy,
    ) -> Self {
        Self {
            public_base_url: trim_url_root(public_base_url.into()),
            resource_policy,
            current_artifacts: None,
            bundle_manifests_by_filename: BTreeMap::new(),
        }
    }

    pub fn set_resource_policy(&mut self, resource_policy: CoreResourcePolicy) {
        self.resource_policy = resource_policy;
    }

    pub fn ingest_resource(
        &mut self,
        resource_id: &str,
        resource_bytes: &[u8],
    ) -> Result<(), String> {
        let payload = std::str::from_utf8(resource_bytes)
            .map_err(|err| format!("publication resource {resource_id} is not utf-8: {err}"))?;
        if resource_id == CURRENT_ARTIFACTS_RESOURCE_ID {
            self.current_artifacts = Some(
                serde_json::from_str::<CurrentArtifactsManifest>(payload)
                    .map_err(|err| format!("failed to decode current_artifacts.json: {err}"))?,
            );
            return Ok(());
        }
        let filename = resource_id
            .strip_prefix("publication/bundle/")
            .ok_or_else(|| format!("unsupported publication resource id: {resource_id}"))?;
        let bundle = serde_json::from_str::<BundleManifest>(payload)
            .map_err(|err| format!("failed to decode bundle {filename}: {err}"))?;
        self.bundle_manifests_by_filename
            .insert(filename.to_string(), bundle);
        Ok(())
    }

    pub fn resolve_package_resource(
        &self,
        package_id: &str,
        member_path: &str,
    ) -> Result<HadOperationOutcome, String> {
        self.resolve_resource(|package| package.id == package_id, member_path)
    }

    pub fn package_member_public_url(
        &self,
        package_id: &str,
        member_path: &str,
    ) -> Result<String, String> {
        if self.resource_policy != CoreResourcePolicy::PublicUnpacked {
            return Err(
                "publication URL resolution requires public_unpacked resource policy".to_string(),
            );
        }
        if let Some(resources) = self.missing_manifest_resources()? {
            let resource_ids = resources
                .into_iter()
                .map(|resource| resource.id)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "publication metadata missing for package URL resolution: {resource_ids}"
            ));
        }
        let package = self.matching_package(|package| package.id == package_id)?;
        self.package_member_address(package, member_path)
    }

    pub fn resolve_family_resource(
        &self,
        family_id: &str,
        member_path: &str,
    ) -> Result<HadOperationOutcome, String> {
        self.resolve_resource(|package| package.family_id == family_id, member_path)
    }

    pub fn resolve_nav_db_artifact_candidates(&self) -> Result<HadOperationOutcome, String> {
        if let Some(resources) = self.missing_manifest_resources()? {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
        let candidates = self
            .bundle_manifests_by_filename
            .values()
            .flat_map(|bundle| bundle.packages.iter())
            .filter(|package| package.family_id == "nav-db")
            .filter(|package| {
                package
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.nav_db_contract_version)
                    == Some(REQUIRED_NAV_DB_CONTRACT_VERSION)
            })
            .map(|package| {
                Ok(NavDbArtifactCandidate {
                    package_id: package.id.clone(),
                    filename: package.filename.clone(),
                    root_source: Some(self.package_member_source(package, "root")?),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let result = serde_json::to_value(candidates).map_err(|err| err.to_string())?;
        Ok(HadOperationOutcome::complete(result))
    }

    pub fn package_resource_requests(
        &self,
        target_resource_id: &str,
        package_id: &str,
        member_path: &str,
        optional: bool,
    ) -> Result<Vec<CoreResourceRequest>, String> {
        self.resource_requests(
            target_resource_id,
            |package| package.id == package_id,
            member_path,
            optional,
        )
    }

    pub fn family_resource_requests(
        &self,
        target_resource_id: &str,
        family_id: &str,
        member_path: &str,
        optional: bool,
    ) -> Result<Vec<CoreResourceRequest>, String> {
        self.resource_requests(
            target_resource_id,
            |package| package.family_id == family_id,
            member_path,
            optional,
        )
    }

    fn resource_requests(
        &self,
        target_resource_id: &str,
        matches_package: impl Fn(&BundlePackageArtifact) -> bool,
        member_path: &str,
        optional: bool,
    ) -> Result<Vec<CoreResourceRequest>, String> {
        if let Some(resources) = self.missing_manifest_resources()? {
            return Ok(resources);
        }
        let package = self.matching_package(matches_package)?;
        Ok(vec![CoreResourceRequest {
            id: target_resource_id.to_string(),
            source: self.package_member_source(package, member_path)?,
            optional,
        }])
    }

    fn resolve_resource(
        &self,
        matches_package: impl Fn(&BundlePackageArtifact) -> bool,
        member_path: &str,
    ) -> Result<HadOperationOutcome, String> {
        if let Some(resources) = self.missing_manifest_resources()? {
            return Ok(HadOperationOutcome::NeedResources { resources });
        }
        if self.resource_policy != CoreResourcePolicy::PublicUnpacked {
            return Err(
                "publication URL resolution requires public_unpacked resource policy".to_string(),
            );
        }
        let package = self.matching_package(matches_package)?;
        let source = self.package_member_source(package, member_path)?;
        let result = serde_json::to_value(PublicationResolvedResource { source })
            .map_err(|err| err.to_string())?;
        Ok(HadOperationOutcome::complete(result))
    }

    fn package_member_source(
        &self,
        package: &BundlePackageArtifact,
        member_path: &str,
    ) -> Result<CoreResourceSource, String> {
        match self.resource_policy {
            CoreResourcePolicy::PublicUnpacked => Ok(CoreResourceSource::PublicUrl {
                url: self.package_member_address(package, member_path)?,
            }),
            CoreResourcePolicy::InstalledPackage => Ok(CoreResourceSource::PackageMember {
                package_id: package.id.clone(),
                filename: package.filename.clone(),
                member_path: member_path.to_string(),
            }),
        }
    }

    fn matching_package(
        &self,
        matches_package: impl Fn(&BundlePackageArtifact) -> bool,
    ) -> Result<&BundlePackageArtifact, String> {
        self.bundle_manifests_by_filename
            .values()
            .flat_map(|bundle| bundle.packages.iter())
            .find(|package| matches_package(package))
            .ok_or_else(|| "package not found in active publication bundles".to_string())
    }

    fn package_member_address(
        &self,
        package: &BundlePackageArtifact,
        member_path: &str,
    ) -> Result<String, String> {
        let package_dir = package.relative_path.strip_suffix(".zip").ok_or_else(|| {
            format!(
                "package {} relative_path is not a zip: {}",
                package.id, package.relative_path
            )
        })?;
        let current_artifacts = self
            .current_artifacts
            .as_ref()
            .ok_or_else(|| "publication current_artifacts missing after load".to_string())?;
        Ok(join_url([
            self.public_base_url.as_str(),
            current_artifacts.artifact_roots.unpacked.as_str(),
            package_dir,
            member_path,
        ]))
    }

    fn missing_manifest_resources(&self) -> Result<Option<Vec<CoreResourceRequest>>, String> {
        let Some(current_artifacts) = self.current_artifacts.as_ref() else {
            return Ok(Some(vec![CoreResourceRequest::public_url(
                CURRENT_ARTIFACTS_RESOURCE_ID,
                join_url([self.public_base_url.as_str(), "current_artifacts.json"]),
                false,
            )]));
        };
        let resources = current_artifacts
            .bundles
            .iter()
            .filter(|bundle| {
                !self
                    .bundle_manifests_by_filename
                    .contains_key(&bundle.filename)
            })
            .map(|bundle| {
                CoreResourceRequest::public_url(
                    format!("publication/bundle/{}", bundle.filename),
                    join_url([
                        self.public_base_url.as_str(),
                        current_artifacts.artifact_roots.packaged.as_str(),
                        bundle.relative_path.as_str(),
                    ]),
                    false,
                )
            })
            .collect::<Vec<_>>();
        if resources.is_empty() {
            Ok(None)
        } else {
            Ok(Some(resources))
        }
    }
}

fn trim_url_root(root: String) -> String {
    let trimmed = root.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed
    }
}

fn join_url<const N: usize>(parts: [&str; N]) -> String {
    parts
        .into_iter()
        .enumerate()
        .map(|(index, part)| {
            if index == 0 {
                part.trim_end_matches('/').to_string()
            } else {
                part.trim_matches('/').to_string()
            }
        })
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn serialize_publication_outcome(outcome: HadOperationOutcome) -> Result<String, String> {
    serde_json::to_string(&outcome).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        package_management::CurrentArtifactsArtifactRoots, CurrentArtifactsBundleRef,
        HadOperationOutcome,
    };

    fn current_artifacts() -> CurrentArtifactsManifest {
        CurrentArtifactsManifest {
            schema_version: Some(1),
            artifact_roots: CurrentArtifactsArtifactRoots {
                packaged: "published_packaged".to_string(),
                unpacked: "published_unpacked".to_string(),
            },
            as_of_date: None,
            as_of_utc: None,
            bundles: vec![CurrentArtifactsBundleRef {
                filename: "bundle_cycle.json".to_string(),
                relative_path: "bundles/bundle_cycle.json".to_string(),
                id: "cycle".to_string(),
                bundle_type: "cycle".to_string(),
                cycle: None,
                cycle_version: None,
                start_valid: None,
                end_valid: None,
                checksum_sha256: None,
                size_bytes: None,
            }],
        }
    }

    fn bundle() -> BundleManifest {
        BundleManifest {
            packages: vec![BundlePackageArtifact {
                id: "nav-db".to_string(),
                family_id: "nav-db".to_string(),
                region_id: None,
                filename: "nav_db_hash.zip".to_string(),
                relative_path: "nav_db_hash.zip".to_string(),
                cycle: None,
                cycle_version: None,
                checksum_sha256: None,
                size_bytes: None,
                effective_date: None,
                expiration_date: None,
                metadata: Some(crate::package_management::BundlePackageMetadata {
                    nav_db_contract_version: Some(REQUIRED_NAV_DB_CONTRACT_VERSION),
                    full_coverage_zoom: None,
                    wide_angle_region_id: None,
                    wide_angle_max_zoom: None,
                    wide_angle: None,
                    min_source_zoom: None,
                    max_source_zoom: None,
                    tile_count: None,
                }),
            }],
        }
    }

    #[test]
    fn resolver_requests_current_artifacts_first() {
        let resolver = PublicationResolver::new("/packages");
        let outcome = resolver
            .resolve_family_resource("nav-db", "root")
            .expect("resolve outcome");
        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("expected resource request");
        };
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "publication/current_artifacts");
        assert_eq!(
            resources[0].source,
            CoreResourceSource::PublicUrl {
                url: "/packages/current_artifacts.json".to_string(),
            }
        );
    }

    #[test]
    fn resolver_requests_bundle_from_current_artifacts_packaged_root() {
        let mut resolver = PublicationResolver::new("/packages");
        resolver
            .ingest_resource(
                "publication/current_artifacts",
                serde_json::to_string(&current_artifacts())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        let outcome = resolver
            .resolve_family_resource("nav-db", "root")
            .expect("resolve outcome");
        let HadOperationOutcome::NeedResources { resources } = outcome else {
            panic!("expected resource request");
        };
        assert_eq!(resources[0].id, "publication/bundle/bundle_cycle.json");
        assert_eq!(
            resources[0].source,
            CoreResourceSource::PublicUrl {
                url: "/packages/published_packaged/bundles/bundle_cycle.json".to_string(),
            }
        );
    }

    #[test]
    fn resolver_resolves_package_resource_from_active_bundle() {
        let mut resolver = PublicationResolver::new("/packages");
        resolver
            .ingest_resource(
                "publication/current_artifacts",
                serde_json::to_string(&current_artifacts())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        resolver
            .ingest_resource(
                "publication/bundle/bundle_cycle.json",
                serde_json::to_string(&bundle()).unwrap().as_bytes(),
            )
            .unwrap();
        let outcome = resolver
            .resolve_family_resource("nav-db", "page_0007")
            .expect("resolve outcome");
        let HadOperationOutcome::Complete { result, .. } = outcome else {
            panic!("expected complete result");
        };
        let resolved: PublicationResolvedResource = serde_json::from_value(result).unwrap();
        assert_eq!(
            resolved.source,
            CoreResourceSource::PublicUrl {
                url: "/packages/published_unpacked/nav_db_hash/page_0007".to_string(),
            }
        );
    }

    #[test]
    fn resolver_exposes_direct_package_member_public_url() {
        let mut resolver = PublicationResolver::new("/packages");
        resolver
            .ingest_resource(
                "publication/current_artifacts",
                serde_json::to_string(&current_artifacts())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        resolver
            .ingest_resource(
                "publication/bundle/bundle_cycle.json",
                serde_json::to_string(&bundle()).unwrap().as_bytes(),
            )
            .unwrap();
        assert_eq!(
            resolver
                .package_member_public_url("nav-db", "page_0007")
                .unwrap(),
            "/packages/published_unpacked/nav_db_hash/page_0007"
        );
    }

    #[test]
    fn resolver_produces_opaque_fetchable_request_for_package_member() {
        let mut resolver = PublicationResolver::new("/packages");
        resolver
            .ingest_resource(
                "publication/current_artifacts",
                serde_json::to_string(&current_artifacts())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        resolver
            .ingest_resource(
                "publication/bundle/bundle_cycle.json",
                serde_json::to_string(&bundle()).unwrap().as_bytes(),
            )
            .unwrap();
        let resources = resolver
            .family_resource_requests("nav/root", "nav-db", "root", false)
            .unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "nav/root");
        assert_eq!(
            resources[0].source,
            CoreResourceSource::PublicUrl {
                url: "/packages/published_unpacked/nav_db_hash/root".to_string(),
            }
        );
    }

    #[test]
    fn resolver_produces_installed_package_request_when_policy_requires_packages() {
        let mut resolver = PublicationResolver::with_resource_policy(
            "/packages",
            CoreResourcePolicy::InstalledPackage,
        );
        resolver
            .ingest_resource(
                "publication/current_artifacts",
                serde_json::to_string(&current_artifacts())
                    .unwrap()
                    .as_bytes(),
            )
            .unwrap();
        resolver
            .ingest_resource(
                "publication/bundle/bundle_cycle.json",
                serde_json::to_string(&bundle()).unwrap().as_bytes(),
            )
            .unwrap();
        let resources = resolver
            .family_resource_requests("nav/root", "nav-db", "root", false)
            .unwrap();
        assert_eq!(
            resources[0].source,
            CoreResourceSource::PackageMember {
                package_id: "nav-db".to_string(),
                filename: "nav_db_hash.zip".to_string(),
                member_path: "root".to_string(),
            }
        );
    }
}

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    BundleManifest, BundlePackageArtifact, CoreResourceRequest, CurrentArtifactsManifest,
    HadOperationOutcome, NavDbArtifactCandidate,
};

const CURRENT_ARTIFACTS_RESOURCE_ID: &str = "publication/current_artifacts";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationResolver {
    public_base_url: String,
    current_artifacts: Option<CurrentArtifactsManifest>,
    bundle_manifests_by_filename: BTreeMap<String, BundleManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicationResolvedResource {
    pub address: String,
}

impl PublicationResolver {
    pub fn new(public_base_url: impl Into<String>) -> Self {
        Self {
            public_base_url: trim_url_root(public_base_url.into()),
            current_artifacts: None,
            bundle_manifests_by_filename: BTreeMap::new(),
        }
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
        let candidates =
            self.bundle_manifests_by_filename
                .values()
                .flat_map(|bundle| bundle.packages.iter())
                .filter(|package| package.family_id == "nav-db")
                .map(|package| {
                    Ok(NavDbArtifactCandidate {
                        package_id: package.id.clone(),
                        filename: package.filename.clone(),
                        root_address: Some(self.package_member_address(
                            |candidate| candidate.id == package.id,
                            "root",
                        )?),
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
        Ok(vec![CoreResourceRequest {
            id: target_resource_id.to_string(),
            address: self.package_member_address(matches_package, member_path)?,
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
        let address = self.package_member_address(matches_package, member_path)?;
        let result = serde_json::to_value(PublicationResolvedResource { address })
            .map_err(|err| err.to_string())?;
        Ok(HadOperationOutcome::complete(result))
    }

    fn package_member_address(
        &self,
        matches_package: impl Fn(&BundlePackageArtifact) -> bool,
        member_path: &str,
    ) -> Result<String, String> {
        let package = self
            .bundle_manifests_by_filename
            .values()
            .flat_map(|bundle| bundle.packages.iter())
            .find(|package| matches_package(package))
            .ok_or_else(|| "package not found in active publication bundles".to_string())?;
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
            return Ok(Some(vec![CoreResourceRequest {
                id: CURRENT_ARTIFACTS_RESOURCE_ID.to_string(),
                address: join_url([self.public_base_url.as_str(), "current_artifacts.json"]),
                optional: false,
            }]));
        };
        let resources = current_artifacts
            .bundles
            .iter()
            .filter(|bundle| {
                !self
                    .bundle_manifests_by_filename
                    .contains_key(&bundle.filename)
            })
            .map(|bundle| CoreResourceRequest {
                id: format!("publication/bundle/{}", bundle.filename),
                address: join_url([
                    self.public_base_url.as_str(),
                    current_artifacts.artifact_roots.packaged.as_str(),
                    bundle.relative_path.as_str(),
                ]),
                optional: false,
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
                metadata: None,
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
        assert_eq!(resources[0].address, "/packages/current_artifacts.json");
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
            resources[0].address,
            "/packages/published_packaged/bundles/bundle_cycle.json"
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
            resolved.address,
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
            resources[0].address,
            "/packages/published_unpacked/nav_db_hash/root"
        );
    }
}

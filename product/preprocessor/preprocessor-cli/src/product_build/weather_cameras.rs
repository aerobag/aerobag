// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, path::Path};

use anyhow::{bail, Context};
use preprocessor_fetch::PrefetchRequest;

pub(super) const DEFAULT_TOKEN_FILE: &str = "/root/aerobag-credentials/faa-weathercams-token";
const CREDENTIAL_HELP: &str = "Request API access from 9-AJO-WCAM-ProgramOffice@faa.gov and sign the MoU they send; see docs/WEATHER_CAMERAS.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WeatherCameraSource {
    OfficialApi,
    Website,
}

pub(super) fn configured_sources(
    value: Option<&str>,
) -> anyhow::Result<&'static [WeatherCameraSource]> {
    use WeatherCameraSource::*;
    match value {
        None | Some("combined") => Ok(&[OfficialApi, Website]),
        Some("official-api") => Ok(&[OfficialApi]),
        Some("website") => Ok(&[Website]),
        Some(_) => {
            bail!("AEROBAG_WEATHER_CAMERA_SOURCE must be combined, official-api, or website")
        }
    }
}

impl WeatherCameraSource {
    pub(super) fn url(self) -> &'static str {
        match self {
            Self::OfficialApi => "https://weathercams.faa.gov/api/redistributable/sites",
            Self::Website => "https://weathercams.faa.gov/api/sites",
        }
    }

    pub(super) fn file_name(self) -> &'static str {
        match self {
            Self::OfficialApi => "redistributable-weather-camera-sites.json",
            Self::Website => "weather-camera-sites.json",
        }
    }

    pub(super) fn request(self, token_file: &Path) -> anyhow::Result<PrefetchRequest> {
        let request = PrefetchRequest::new(self.url()).with_logical_file_name(self.file_name());
        match self {
            Self::OfficialApi => {
                let token = fs::read_to_string(token_file).with_context(|| {
                    format!(
                        "failed to read FAA weather-camera token {}; set AEROBAG_WEATHER_CAMERA_TOKEN_FILE to the raw token file. {CREDENTIAL_HELP}",
                        token_file.display()
                    )
                })?;
                let token = token.trim();
                if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_graphic()) {
                    bail!(
                        "FAA weather-camera token {} must contain one non-empty raw token, without the Bearer prefix. {CREDENTIAL_HELP}",
                        token_file.display()
                    );
                }
                Ok(request.with_header("Authorization", format!("Bearer {token}")))
            }
            Self::Website => {
                // The website inventory supplies Canadian sites absent from the official API.
                // Its undocumented API requires this same-origin header.
                Ok(request.with_header("Referer", "https://weathercams.faa.gov/"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_camera_sources_default_to_both_with_official_precedence() {
        let sources = configured_sources(None).unwrap();
        assert_eq!(
            sources,
            &[
                WeatherCameraSource::OfficialApi,
                WeatherCameraSource::Website
            ]
        );
        assert_eq!(configured_sources(Some("combined")).unwrap(), sources);
        assert_eq!(
            configured_sources(Some("official-api")).unwrap(),
            &sources[..1]
        );
        assert_eq!(
            sources[0].url(),
            "https://weathercams.faa.gov/api/redistributable/sites"
        );
        assert_ne!(sources[0].file_name(), sources[1].file_name());
        assert!(configured_sources(Some("auto")).is_err());
        assert!(configured_sources(Some("")).is_err());
    }

    #[test]
    fn weather_camera_api_reads_raw_token_without_changing_cache_identity() {
        let directory = tempfile::tempdir().unwrap();
        let token_file = directory.path().join("token");
        fs::write(&token_file, "test-only-token\n").unwrap();
        let request = WeatherCameraSource::OfficialApi
            .request(&token_file)
            .unwrap();
        assert_eq!(request.headers["Authorization"], "Bearer test-only-token");
        assert!(!request.headers.contains_key("Referer"));
        assert_eq!(request.cache_key, WeatherCameraSource::OfficialApi.url());
        assert_eq!(
            request.logical_file_name.as_deref(),
            Some(WeatherCameraSource::OfficialApi.file_name())
        );
        assert!(!format!("{request:?}").contains("test-only-token"));
    }

    #[test]
    fn weather_camera_api_rejects_missing_or_invalid_credentials_without_switching_sources() {
        let directory = tempfile::tempdir().unwrap();
        let token_file = directory.path().join("token");
        let error = WeatherCameraSource::OfficialApi
            .request(&token_file)
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("9-AJO-WCAM-ProgramOffice@faa.gov"));
        assert!(error.to_string().contains("docs/WEATHER_CAMERAS.md"));
        for token in [
            "",
            " \n",
            "Bearer test-only-token",
            "test-only-token\nX: injected",
        ] {
            fs::write(&token_file, token).unwrap();
            let error = WeatherCameraSource::OfficialApi
                .request(&token_file)
                .unwrap_err();
            assert!(error.to_string().contains("raw token"));
            assert!(!error.to_string().contains("test-only-token"));
        }
    }

    #[test]
    fn weather_camera_website_source_is_explicit_and_needs_no_token() {
        let source = configured_sources(Some("website")).unwrap()[0];
        let request = source.request(Path::new("/does-not-exist/token")).unwrap();
        assert_eq!(request.url, "https://weathercams.faa.gov/api/sites");
        assert_eq!(request.headers["Referer"], "https://weathercams.faa.gov/");
        assert!(!request.headers.contains_key("Authorization"));
        assert_ne!(request.cache_key, WeatherCameraSource::OfficialApi.url());
    }
}

// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, ensure, Context};
use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use preprocessor_fetch::{
    fetch_http_response, prefetch_archives_with_provenance, FetchCacheMode, NetworkTimeouts,
    PrefetchRequest,
};
use serde_json::json;

use crate::products::{live_feed_request, run_gzip_decompress, LiveFeedFetchConfig};

const BULK_URL: &str = "https://aviationweather.gov/data/cache/aircraftreports.cache.xml.gz";
const API_URL: &str = "https://aviationweather.gov/api/data/pirep";
const RESULT_LIMIT: usize = 400;
const WINDOW_MINUTES: i64 = 90;
const MAX_QUERIES: usize = 63;
const API_BUDGET: Duration = Duration::from_secs(120);
const REQUEST_LIMIT: Duration = Duration::from_secs(20);
const REQUEST_SPACING: Duration = Duration::from_secs(1);
const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Window {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl Window {
    fn url(self) -> String {
        // Whole-minute boundaries plus one minute of lower-bound overlap avoid
        // AWC's rounding of fractional-hour ages. Client ownership is [start,end).
        let hours = (self.end - self.start).num_minutes() as f64 / 60.0 + 1.0 / 60.0;
        format!(
            "{API_URL}?format=xml&age={hours:.9}&date={}&bbox=-90,-180,90,180",
            self.end.to_rfc3339_opts(SecondsFormat::Secs, true)
        )
    }
}

struct SourceReport {
    observed: DateTime<Utc>,
    xml: String,
}

fn parse_response(xml: &str) -> anyhow::Result<Vec<SourceReport>> {
    let document = roxmltree::Document::parse(xml).context("invalid AWC XML response")?;
    let root = document.root_element();
    ensure!(
        root.has_tag_name("response"),
        "AWC response envelope is missing"
    );
    if let Some(error) = root.descendants().find(|node| node.has_tag_name("error")) {
        bail!(
            "AWC source error: {}",
            error.text().unwrap_or("unspecified error")
        );
    }
    let data = root
        .children()
        .filter(|node| node.has_tag_name("data"))
        .collect::<Vec<_>>();
    ensure!(
        data.len() == 1,
        "AWC response must have exactly one data element"
    );
    let expected: usize = data[0]
        .attribute("num_results")
        .context("AWC response is missing num_results")?
        .parse()
        .context("AWC num_results is invalid")?;
    let nodes = data[0]
        .children()
        .filter(|node| node.is_element())
        .collect::<Vec<_>>();
    ensure!(
        nodes.len() == expected,
        "AWC response count mismatch: expected {expected}, got {}",
        nodes.len()
    );
    nodes
        .into_iter()
        .map(|node| {
            ensure!(
                node.has_tag_name("AircraftReport") && node.tag_name().namespace().is_none(),
                "unexpected AWC report element"
            );
            let times = node
                .children()
                .filter(|child| child.has_tag_name("observation_time"))
                .collect::<Vec<_>>();
            ensure!(
                times.len() == 1,
                "AWC report must have exactly one observation time"
            );
            let observed = DateTime::parse_from_rfc3339(times[0].text().unwrap_or(""))
                .context("invalid AWC report observation time")?
                .with_timezone(&Utc);
            ensure!(
                node.children().any(|child| child.has_tag_name("raw_text")
                    && child.text().is_some_and(|text| !text.trim().is_empty())),
                "AWC report is missing raw text"
            );
            // Preserve source spelling, entities, coordinate precision and duplicate
            // records: they participate in the existing PIREP record identities.
            Ok(SourceReport {
                observed,
                xml: xml[node.range()].to_owned(),
            })
        })
        .collect()
}

fn select_source(
    bulk: impl FnOnce() -> anyhow::Result<String>,
    api: impl FnOnce() -> anyhow::Result<String>,
) -> anyhow::Result<(String, Option<String>)> {
    match bulk().and_then(|xml| {
        parse_response(&xml)?;
        Ok(xml)
    }) {
        Ok(xml) => Ok((xml, None)),
        Err(error) => {
            let error = format!("{error:#}");
            eprintln!("PIREP bulk source failed; using bounded AWC API queries: {error}");
            let xml = api().with_context(|| {
                format!("PIREP bulk source failed ({error}); API fallback failed")
            })?;
            Ok((xml, Some(error)))
        }
    }
}

fn collect_api(
    cutoff: DateTime<Utc>,
    max_queries: usize,
    mut fetch: impl FnMut(Window, usize) -> anyhow::Result<(u16, String)>,
) -> anyhow::Result<(String, usize)> {
    // Exclude the current partial minute. AWC rounds fractional-hour ages, so
    // every ownership boundary must be a whole minute, including the cutoff.
    let cutoff = cutoff.with_second(0).unwrap().with_nanosecond(0).unwrap();
    let mut pending = vec![Window {
        start: cutoff - chrono::Duration::minutes(WINDOW_MINUTES),
        end: cutoff,
    }];
    let mut reports = Vec::new();
    let mut queries = 0;
    while let Some(window) = pending.pop() {
        ensure!(
            queries < max_queries,
            "PIREP API query budget exhausted before complete coverage"
        );
        queries += 1;
        let (status, body) = fetch(window, queries)?;
        let rows = match status {
            200 => parse_response(&body)?,
            204 => {
                ensure!(body.is_empty(), "AWC HTTP 204 response contains data");
                Vec::new()
            }
            other => bail!("AWC API returned HTTP {other}"),
        };
        for row in &rows {
            ensure!(
                row.observed >= window.start - chrono::Duration::minutes(1)
                    && row.observed <= window.end,
                "AWC report lies outside requested time range"
            );
        }
        if rows.len() >= RESULT_LIMIT {
            let minutes = (window.end - window.start).num_minutes();
            ensure!(
                minutes > 1,
                "PIREP API still capped in a one-minute window; complete coverage unavailable"
            );
            let middle = window.start + chrono::Duration::minutes(minutes / 2);
            pending.push(Window {
                start: middle,
                end: window.end,
            });
            pending.push(Window {
                start: window.start,
                end: middle,
            });
            continue;
        }
        reports.extend(
            rows.into_iter()
                .filter(|row| row.observed >= window.start && row.observed < window.end),
        );
    }
    // Nothing reaches the builder/publisher until every leaf has succeeded.
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><response><data num_results=\"{}\">",
        reports.len()
    );
    for report in reports {
        xml.push_str(&report.xml);
    }
    xml.push_str("</data></response>");
    Ok((xml, queries))
}

pub(crate) fn fetch_pirep_xml(
    config: &LiveFeedFetchConfig,
    input: &Path,
    provenance: &Path,
    cutoff: DateTime<Utc>,
) -> anyhow::Result<PathBuf> {
    let mut api_queries = None;
    let (xml, bulk_error) = select_source(
        || {
            prefetch_archives_with_provenance(
                &[live_feed_request(BULK_URL)
                    .with_timeouts(NetworkTimeouts {
                        connect: Duration::from_secs(10),
                        total: REQUEST_LIMIT,
                    })
                    .with_logical_file_name("aircraftreports.cache.xml.gz")],
                input,
                config.fetch_jobs,
                config.fetch_cache.as_ref(),
                provenance,
                "pireps",
            )?;
            run_gzip_decompress(&input.join("aircraftreports.cache.xml.gz"))?;
            fs::read_to_string(input.join("aircraftreports.cache.xml"))
                .context("failed to read PIREP bulk response")
        },
        || {
            ensure!(
                !config
                    .fetch_cache
                    .as_ref()
                    .is_some_and(|cache| cache.mode == FetchCacheMode::Offline),
                "PIREP API fallback is unavailable in offline fetch mode"
            );
            let started = Instant::now();
            let mut last_request: Option<Instant> = None;
            let (xml, queries) = collect_api(cutoff, MAX_QUERIES, |window, index| {
                if let Some(last) = last_request {
                    if let Some(wait) = REQUEST_SPACING.checked_sub(last.elapsed()) {
                        thread::sleep(wait);
                    }
                }
                let remaining = API_BUDGET
                    .checked_sub(started.elapsed())
                    .context("PIREP API deadline exceeded before complete coverage")?;
                let timeout = remaining.min(REQUEST_LIMIT);
                let url = window.url();
                let request = PrefetchRequest::new(&url)
                    .with_header("User-Agent", "Aerobag-live-feeds/1")
                    .with_timeouts(NetworkTimeouts {
                        connect: timeout.min(Duration::from_secs(10)),
                        total: timeout,
                    });
                let destination = input.join("api").join(format!("query-{index:02}.xml"));
                last_request = Some(Instant::now());
                let status = fetch_http_response(&request, &destination)?;
                ensure!(
                    started.elapsed() < API_BUDGET,
                    "PIREP API deadline exceeded before complete coverage"
                );
                ensure!(
                    fs::metadata(&destination)?.len() <= MAX_RESPONSE_BYTES,
                    "PIREP API response exceeds size limit"
                );
                let body = fs::read_to_string(&destination)?;
                fs::create_dir_all(provenance.join("api"))?;
                fs::write(
                    provenance
                        .join("api")
                        .join(format!("query-{index:02}.json")),
                    serde_json::to_vec_pretty(&json!({ "url": url, "http_status": status,
                    "start": window.start, "end": window.end, "file": destination,
                    "sha256": crate::engine::sha256_hex(body.as_bytes()) }))?,
                )?;
                Ok((status, body))
            })?;
            ensure!(
                started.elapsed() < API_BUDGET,
                "PIREP API deadline exceeded before complete coverage"
            );
            api_queries = Some(queries);
            Ok(xml)
        },
    )?;
    fs::create_dir_all(provenance)?;
    fs::write(
        provenance.join("source-selection.json"),
        serde_json::to_vec_pretty(&json!({
            "source": if bulk_error.is_some() { "awc-api" } else { "awc-bulk" },
            "bulk_error": bulk_error, "cutoff": cutoff, "api_queries": api_queries,
        }))?,
    )?;
    let destination = input.join("pireps-source.xml");
    let temporary = input.join("pireps-source.xml.part");
    fs::write(&temporary, xml)?;
    fs::rename(temporary, &destination)?;
    Ok(destination)
}

#[cfg(test)]
mod tests;

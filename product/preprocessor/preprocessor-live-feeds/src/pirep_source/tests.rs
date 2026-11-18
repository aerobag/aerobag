// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;
use chrono::TimeZone;

fn cutoff() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 18, 19, 0, 0).unwrap()
}

fn row(time: DateTime<Utc>, raw: &str) -> String {
    format!("<AircraftReport><observation_time>{}</observation_time><latitude>47.50</latitude><longitude>-122.30</longitude><report_type>PIREP</report_type><raw_text>{raw}</raw_text></AircraftReport>",
        time.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn response(rows: &[String]) -> String {
    format!(
        "<response><errors/><data num_results=\"{}\">{}</data></response>",
        rows.len(),
        rows.join("")
    )
}

#[test]
fn valid_bulk_data_never_calls_api_and_errors_trigger_fallback() -> anyhow::Result<()> {
    let bulk = response(&[row(cutoff(), "SEA UA /IC LGT")]);
    let (actual, error) =
        select_source(|| Ok(bulk.clone()), || panic!("API called for valid bulk"))?;
    assert_eq!(actual, bulk);
    assert!(error.is_none());
    for source in [
        "<response><errors><error>Error processing data</error></errors></response>",
        "<html>unavailable</html>",
        "<response><data/></response>",
    ] {
        let (actual, error) = select_source(|| Ok(source.into()), || Ok(bulk.clone()))?;
        assert_eq!(actual, bulk);
        assert!(error.is_some());
    }
    let (actual, error) = select_source(|| bail!("bulk download failed"), || Ok(bulk.clone()))?;
    assert_eq!(actual, bulk);
    assert!(error.unwrap().contains("bulk download failed"));
    let error = select_source(|| bail!("bulk failed"), || bail!("API failed")).unwrap_err();
    let detail = format!("{error:#}");
    assert!(detail.contains("bulk failed") && detail.contains("API failed"));
    Ok(())
}

#[test]
fn capped_parents_are_discarded_and_boundaries_have_one_owner() -> anyhow::Result<()> {
    let end = cutoff();
    let start = end - chrono::Duration::minutes(90);
    let middle = start + chrono::Duration::minutes(45);
    let duplicate = row(
        middle + chrono::Duration::minutes(1),
        "TWO DISTINCT DUPLICATE REPORTS",
    );
    let mut seen = Vec::new();
    let (xml, count) = collect_api(end, MAX_QUERIES, |window, _| {
        seen.push(window);
        Ok((
            200,
            if seen.len() == 1 {
                response(&vec![row(end, "CAPPED PARENT MUST NOT BE USED"); 400])
            } else if window.end == middle {
                response(&[row(start, "FIRST"), row(middle, "BOUNDARY")])
            } else {
                response(&[
                    row(middle - chrono::Duration::minutes(1), "OVERLAP"),
                    row(middle, "BOUNDARY"),
                    duplicate.clone(),
                    duplicate.clone(),
                    row(end, "CUTOFF"),
                ])
            },
        ))
    })?;
    assert_eq!(count, 3);
    assert_eq!(
        seen,
        vec![
            Window { start, end },
            Window { start, end: middle },
            Window { start: middle, end }
        ]
    );
    let rows = parse_response(&xml)?;
    assert_eq!(rows.len(), 4);
    assert_eq!(xml.matches("BOUNDARY").count(), 1);
    assert_eq!(xml.matches("TWO DISTINCT DUPLICATE REPORTS").count(), 2);
    for excluded in ["CAPPED PARENT", "OVERLAP", "CUTOFF"] {
        assert!(!xml.contains(excluded));
    }
    assert!(xml.contains("<latitude>47.50</latitude>"));
    assert!(seen[1].url().contains("bbox=-90,-180,90,180"));
    assert!(seen[1].url().contains("date=2026-11-18T18:15:00Z"));
    Ok(())
}

#[test]
fn malformed_error_missing_and_count_mismatch_responses_are_fatal() {
    for body in ["", "<response>", "<html/>", "<response/>",
        "<response><data num_results=\"0\"/><data num_results=\"0\"/></response>",
        "<response><errors><error>upstream failed</error></errors><data num_results=\"0\"/></response>",
        "<response><data num_results=\"1\"/></response>",
        "<response><data num_results=\"0\"><AircraftReport/></data></response>",
        "<response><data num_results=\"1\"><AircraftReport><raw_text>X</raw_text></AircraftReport></data></response>"] {
        assert!(collect_api(cutoff(), MAX_QUERIES, |_, _| Ok((200, body.into()))).is_err(), "{body}");
    }
}

#[test]
fn no_content_is_distinct_from_empty_success_or_http_failure() -> anyhow::Result<()> {
    let (xml, queries) = collect_api(cutoff(), MAX_QUERIES, |_, _| Ok((204, String::new())))?;
    assert!(parse_response(&xml)?.is_empty());
    assert_eq!(queries, 1);
    for (status, body) in [(200, ""), (204, "unexpected body"), (500, ""), (429, "")] {
        assert!(collect_api(cutoff(), MAX_QUERIES, |_, _| Ok((status, body.into()))).is_err());
    }
    Ok(())
}

#[test]
fn failed_late_window_never_returns_a_partial_snapshot() {
    let mut calls = 0;
    let result = collect_api(cutoff(), MAX_QUERIES, |window, _| {
        calls += 1;
        match calls {
            1 => Ok((200, response(&vec![row(window.end, "parent"); 400]))),
            2 => Ok((200, response(&[row(window.start, "good first leaf")]))),
            _ => bail!("last window failed"),
        }
    });
    assert!(format!("{:#}", result.unwrap_err()).contains("last window failed"));
    assert_eq!(calls, 3);
}

#[test]
fn query_budget_and_dense_minute_fail_without_partial_success() {
    for limit in [1, MAX_QUERIES] {
        let mut calls = 0;
        let result = collect_api(cutoff(), limit, |window, _| {
            calls += 1;
            Ok((200, response(&vec![row(window.start, "dense"); 400])))
        });
        let error = format!("{:#}", result.unwrap_err());
        assert!(error.contains(if limit == 1 {
            "query budget"
        } else {
            "one-minute window"
        }));
        assert!(calls <= limit);
    }
}

#[test]
fn ignored_time_constraints_do_not_look_like_an_empty_window() {
    let stale = row(cutoff() - chrono::Duration::hours(12), "wrong window");
    let error = collect_api(cutoff(), MAX_QUERIES, |_, _| {
        Ok((200, response(&[stale.clone()])))
    })
    .unwrap_err();
    assert!(error.to_string().contains("outside requested time range"));
}

#[test]
fn fractional_cutoff_uses_complete_minutes() -> anyhow::Result<()> {
    let end = cutoff() + chrono::Duration::milliseconds(59_123);
    let (_, queries) = collect_api(end, MAX_QUERIES, |window, _| {
        assert_eq!(window.end, cutoff());
        assert_eq!(window.start, cutoff() - chrono::Duration::minutes(90));
        assert!(window.url().contains("age=1.516666667"));
        Ok((204, String::new()))
    })?;
    assert_eq!(queries, 1);
    Ok(())
}

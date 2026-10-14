// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::{ensure, Context};
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
use roxmltree::Node;

use crate::{ParsedTfrArea, StructuredTfrPoint, StructuredTfrScheduleFragment};

pub(crate) fn text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.children()
        .find(|n| n.has_tag_name(name))
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn instant(value: &str) -> anyhow::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&Utc))
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").map(|t| t.and_utc()))
        .with_context(|| format!("invalid FAA TFR timestamp {value:?}"))
}

fn limit(area: Node<'_, '_>, side: &str) -> anyhow::Result<(String, String)> {
    let value = text(area, &format!("valDistVer{side}")).unwrap_or_default();
    let unit = text(area, &format!("uomDistVer{side}")).unwrap_or_default();
    let datum = text(area, &format!("codeDistVer{side}")).unwrap_or_default();
    if value == "0" && datum == "HEI" {
        return Ok(("SFC".into(), String::new()));
    }
    let unit = match (unit, datum) {
        ("FL", _) => "FL".into(),
        ("FT", "HEI") => "FT AGL".into(),
        ("FT", "ALT") => "FT MSL".into(),
        (_, "") => unit.into(),
        _ => anyhow::bail!("unsupported TFR altitude unit/datum {unit}/{datum}"),
    };
    Ok((value.into(), unit))
}

// Compile FAA area schedules into ordered effective/expires UTC pairs. The
// wire already carries these fragments; clients need no FAA recurrence rules.
fn schedules(
    area: Node<'_, '_>,
    notice: Node<'_, '_>,
) -> anyhow::Result<Vec<StructuredTfrScheduleFragment>> {
    let schedules = area
        .children()
        .filter(|n| n.has_tag_name("ScheduleGroup"))
        .collect::<Vec<_>>();
    let mut windows = Vec::new();
    let mut days = Vec::new();
    for day in area.children().filter(|n| n.has_tag_name("dayCode")) {
        let day = day.text().unwrap_or_default().trim();
        if day.eq_ignore_ascii_case("Daily") {
            continue;
        }
        let index = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ]
        .iter()
        .position(|name| name.eq_ignore_ascii_case(day))
        .with_context(|| format!("unknown TFR weekday {day:?}"))?;
        days.push(index as u32);
    }
    for schedule in schedules
        .iter()
        .copied()
        .chain(schedules.is_empty().then_some(notice))
    {
        let start = text(schedule, "dateEffective")
            .or_else(|| text(notice, "dateEffective"))
            .map(instant)
            .transpose()?;
        let end = text(schedule, "dateExpire")
            .or_else(|| text(notice, "dateExpire"))
            .map(instant)
            .transpose()?;
        ensure!(
            start.zip(end).is_none_or(|(a, b)| a < b),
            "TFR effective interval is inverted"
        );
        if text(schedule, "isTimeSeparate") == Some("TRUE") || !days.is_empty() {
            let start = start.context("recurring TFR schedule has no start")?;
            let end = end.context("recurring TFR schedule has no end")?;
            ensure!(
                (end - start).num_days() <= 3660,
                "TFR recurrence exceeds ten years"
            );
            let daily_start =
                instant(text(schedule, "startTime").context("TFR daily start missing")?)?.time();
            let daily_end =
                instant(text(schedule, "endTime").context("TFR daily end missing")?)?.time();
            // Include the previous day's window when it crosses midnight.
            let mut day = start
                .date_naive()
                .pred_opt()
                .context("TFR date underflow")?;
            while day <= end.date_naive() {
                if days.is_empty() || days.contains(&day.weekday().num_days_from_monday()) {
                    let a = day.and_time(daily_start).and_utc();
                    let mut b = day.and_time(daily_end).and_utc();
                    if b <= a {
                        b += Duration::days(1);
                    }
                    if a.max(start) < b.min(end) {
                        windows.push((Some(a.max(start)), Some(b.min(end))));
                    }
                }
                day = day.succ_opt().context("TFR date overflow")?;
            }
        } else {
            windows.push((start, end));
        }
    }
    windows.sort();
    windows.dedup();
    ensure!(
        !windows.is_empty(),
        "TFR recurrence has no window in its effective interval"
    );
    let mut fragments = Vec::new();
    for (start, end) in windows {
        if let Some(start) = start {
            fragments.push(StructuredTfrScheduleFragment {
                kind: "effective".into(),
                value_utc: start.to_rfc3339(),
            });
        }
        if let Some(end) = end {
            fragments.push(StructuredTfrScheduleFragment {
                kind: "expires".into(),
                value_utc: end.to_rfc3339(),
            });
        }
    }
    Ok(fragments)
}

pub(crate) fn parse(notice: Node<'_, '_>, notam_id: &str) -> anyhow::Result<Vec<ParsedTfrArea>> {
    let mut result = Vec::new();
    for group in notice
        .descendants()
        .filter(|n| n.has_tag_name("TFRAreaGroup"))
    {
        let Some(boundary) = group.children().find(|n| n.has_tag_name("abdMergedArea")) else {
            // Special notices without a drawable area are still valid notice text.
            continue;
        };
        let area = group
            .children()
            .find(|n| n.has_tag_name("aseTFRArea"))
            .context("TFR boundary lacks its area")?;
        let id = area
            .children()
            .find(|n| n.has_tag_name("AseUid"))
            .and_then(|n| text(n, "codeId"));
        let boundary_id = boundary
            .children()
            .find(|n| n.has_tag_name("AbdUid"))
            .and_then(|n| n.children().find(|n| n.has_tag_name("AseUid")))
            .and_then(|n| text(n, "codeId"));
        ensure!(
            id == boundary_id,
            "TFR boundary identity differs from its area"
        );
        let polygon = boundary
            .children()
            .filter(|n| n.has_tag_name("Avx"))
            .map(|vertex| {
                ensure!(
                    text(vertex, "codeType").is_none_or(|kind| kind == "GRC"),
                    "TFR merged boundary contains an unexpanded arc"
                );
                let lat = crate::parse_geo_value(
                    text(vertex, "geoLat").context("TFR vertex lacks latitude")?,
                )?;
                let lon = crate::parse_geo_value(
                    text(vertex, "geoLong").context("TFR vertex lacks longitude")?,
                )?;
                ensure!(
                    lat.is_finite()
                        && lon.is_finite()
                        && (-90.0..=90.0).contains(&lat)
                        && (-180.0..=180.0).contains(&lon),
                    "TFR vertex is not lon/lat"
                );
                Ok(StructuredTfrPoint { lat, lon })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        ensure!(polygon.len() >= 3, "TFR area has fewer than three vertices");
        let (lower_value_text, lower_unit) = limit(area, "Lower")?;
        let (upper_value_text, upper_unit) = limit(area, "Upper")?;
        let schedule_fragments = schedules(area, notice)?;
        let summary_text = crate::build_tfr_summary_text(
            &schedule_fragments,
            &upper_value_text,
            &upper_unit,
            &lower_value_text,
            &lower_unit,
            &polygon,
        )?;
        result.push(ParsedTfrArea {
            notam_id: notam_id.into(),
            source_area_id: id.map(str::to_owned),
            area_index: 0,
            schedule_fragments,
            lower_value_text,
            lower_unit,
            upper_value_text,
            upper_unit,
            polygon,
            summary_text,
        });
    }
    let mut unique = std::collections::BTreeMap::new();
    for area in result {
        let id = crate::structured_tfr_area_id(&area)?;
        if let Some(previous) = unique.insert(id.clone(), area.clone()) {
            ensure!(previous == area, "conflicting TFR XML area {id}");
        }
    }
    Ok(unique.into_values().collect())
}

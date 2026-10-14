// SPDX-FileCopyrightText: 2026 Aerobag contributors
// SPDX-License-Identifier: AGPL-3.0-or-later

use super::*;

fn group(id: &str, lower: &str, datum: &str, schedule: &str) -> String {
    format!(
        r#"<TFRAreaGroup><aseTFRArea>
      <AseUid><codeId>{id}</codeId></AseUid>
      <valDistVerLower>{lower}</valDistVerLower><uomDistVerLower>FT</uomDistVerLower>
      <codeDistVerLower>{datum}</codeDistVerLower>
      <valDistVerUpper>180</valDistVerUpper><uomDistVerUpper>FL</uomDistVerUpper>
      <codeDistVerUpper>ALT</codeDistVerUpper>{schedule}
    </aseTFRArea><abdMergedArea><AbdUid><AseUid><codeId>{id}</codeId></AseUid></AbdUid>
      <Avx><codeType>GRC</codeType><geoLat>48N</geoLat><geoLong>114W</geoLong></Avx>
      <Avx><codeType>GRC</codeType><geoLat>48.1N</geoLat><geoLong>114W</geoLong></Avx>
      <Avx><codeType>GRC</codeType><geoLat>48N</geoLat><geoLong>114.1W</geoLong></Avx>
    </abdMergedArea></TFRAreaGroup>"#
    )
}

fn notice(groups: &[String]) -> String {
    format!(
        r#"<XNOTAM-Update><Not>
      <NotUid><dateIndexYear>2026</dateIndexYear><noSeqNo>8923</noSeqNo></NotUid>
      <dateEffective>2026-10-14T00:00:00</dateEffective>
      <dateExpire>2026-10-17T23:00:00</dateExpire>
      <txtDescrTraditional>SEE FDC 6/1234. AREA A SFC-2400FT. AREA B 2400FT-FL180.</txtDescrTraditional>
      <TfrNot>{}</TfrNot></Not></XNOTAM-Update>"#,
        groups.join("")
    )
}

fn interval(start: &str, end: &str) -> String {
    format!("<ScheduleGroup><isTimeSeparate>FALSE</isTimeSeparate><dateEffective>{start}</dateEffective><dateExpire>{end}</dateExpire></ScheduleGroup>")
}

fn input(root: &Path) -> PathBuf {
    let dir = root.join("input");
    fs::create_dir_all(dir.join("details")).unwrap();
    fs::write(
        dir.join("list.json"),
        r#"[{"notam_id":"6/8923"},{"notam_id":"6/1234"}]"#,
    )
    .unwrap();
    let features = ["6/8923", "6/1234"].map(|id| serde_json::json!({
        "properties": {"NOTAM_KEY": id},
        "geometry": {"type":"Polygon", "coordinates":[[[-114.0,48.0],[-114.0,48.1],[-114.1,48.0]]]}
    }));
    fs::write(
        dir.join("graphics.geojson"),
        serde_json::to_vec(&serde_json::json!({"features":features})).unwrap(),
    )
    .unwrap();
    dir
}

fn publish(dir: &Path) -> Value {
    let result = build_tfr_dataset(&BuildTfrRequest {
        input_dir: dir.into(),
        output_dir: dir.parent().unwrap().join("out"),
        version_label: "test".into(),
        generated_at_utc: DateTime::UNIX_EPOCH,
        notams_by_fdc_id: BTreeMap::new(),
    })
    .unwrap();
    serde_json::from_slice(&fs::read(result.structured_json_path).unwrap()).unwrap()
}

#[test]
fn tfr_scoped_areas_preserve_ownership_and_positive_coverage() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let a = group(
        "A",
        "0",
        "HEI",
        &interval("2026-10-14T22:30:00", "2026-10-15T02:30:00"),
    );
    let b = group(
        "B",
        "2400",
        "ALT",
        &interval("2026-10-15T01:00:00", "2026-10-15T04:00:00"),
    );
    fs::write(dir.join("details/6_8923.xml"), notice(&[a, b])).unwrap();
    let state = publish(&dir);
    let areas = state["areas"].as_array().unwrap();
    assert_eq!(
        areas.len(),
        3,
        "two XML areas, including identical footprints, plus the unfetched WFS area"
    );
    let scoped = areas
        .iter()
        .filter(|a| a["notam_id"] == "6/8923")
        .collect::<Vec<_>>();
    assert_eq!(scoped.len(), 2);
    let elevated = scoped
        .iter()
        .find(|a| a["lower_limit"]["value_text"] == "2400")
        .unwrap();
    assert_eq!(elevated["lower_limit"]["unit"], "FT MSL");
    assert_eq!(elevated["upper_limit"]["unit"], "FL");
    assert_eq!(
        elevated["schedule_fragments"][0]["value_utc"],
        "2026-10-15T01:00:00+00:00"
    );
    assert_eq!(
        elevated["schedule_fragments"][1]["value_utc"],
        "2026-10-15T04:00:00+00:00"
    );
    assert!(scoped
        .iter()
        .all(|a| a["notam"]["text"].as_str().unwrap().contains("SEE FDC")));
    let pending = areas.iter().find(|a| a["notam_id"] == "6/1234").unwrap();
    assert!(
        pending.get("notam").is_none(),
        "a prose reference must not take ownership"
    );
    assert_eq!(
        pending["lower_limit"]["value_text"], "",
        "no guessed altitude while detail is pending"
    );
}

#[test]
fn tfr_scoped_areas_ignore_reordering_and_exact_duplicates() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let a = group("A", "0", "HEI", "");
    let b = group("B", "2400", "ALT", "");
    fs::write(
        dir.join("details/6_8923.xml"),
        notice(&[a.clone(), b.clone()]),
    )
    .unwrap();
    let expected = publish(&dir);
    assert_eq!(expected["area_group_count"], 3);
    fs::write(dir.join("details/6_8923.xml"), notice(&[b, a.clone(), a])).unwrap();
    assert_eq!(publish(&dir), expected);
}

#[test]
fn tfr_scoped_weekday_and_overnight_schedules_keep_every_window() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let schedule = r#"<isScheduledTfrArea>TRUE</isScheduledTfrArea><dayCode>Thursday</dayCode>
      <ScheduleGroup><isTimeSeparate>TRUE</isTimeSeparate>
      <dateEffective>2026-10-14T00:00:00</dateEffective><dateExpire>2026-10-17T23:00:00</dateExpire>
      <startTime>2026-06-01T22:00:00</startTime><endTime>2026-07-01T02:00:00</endTime></ScheduleGroup>"#;
    let a = group("A", "1000", "HEI", schedule);
    let b = group(
        "B",
        "0",
        "ALT",
        &(interval("2026-10-14T20:00:00", "2026-10-14T21:00:00")
            + &interval("2026-10-16T20:00:00", "2026-10-16T21:00:00")),
    );
    fs::write(dir.join("details/6_8923.xml"), notice(&[a, b])).unwrap();
    let state = publish(&dir);
    let areas = state["areas"].as_array().unwrap();
    let a = areas
        .iter()
        .find(|a| a["lower_limit"]["unit"] == "FT AGL")
        .unwrap();
    assert_eq!(
        a["schedule_fragments"],
        serde_json::json!([
          {"kind":"effective","value_utc":"2026-10-15T22:00:00+00:00"},
          {"kind":"expires","value_utc":"2026-10-16T02:00:00+00:00"}
        ])
    );
    let b = areas
        .iter()
        .find(|a| a["lower_limit"]["unit"] == "FT MSL")
        .unwrap();
    assert_eq!(b["schedule_fragments"].as_array().unwrap().len(), 4);
    assert_eq!(
        b["lower_limit"]["value_text"], "0",
        "zero MSL is not surface"
    );
}

#[test]
fn tfr_scoped_cache_warm_restart_and_cold_publish_identical_current_areas() {
    use crate::tfr_detail_backfill::TfrDetailBackfillStore;
    let warm_root = tempfile::tempdir().unwrap();
    let cold_root = tempfile::tempdir().unwrap();
    let warm = TfrDetailBackfillStore::new(warm_root.path().join("cache"));
    let old = BTreeMap::from([("6/8923".into(), "old".into())]);
    let current = BTreeMap::from([("6/8923".into(), "current".into())]);
    warm.reconcile_desired_tfrs(&old, 1).unwrap();
    let old_target = warm.due_fetch_targets(1).unwrap().remove(0);
    warm.record_success(&old_target, &notice(&[group("OLD", "100", "HEI", "")]))
        .unwrap();
    warm.reconcile_desired_tfrs(&current, 1).unwrap();
    let dir = input(warm_root.path());
    warm.copy_current_details_to(&current, &dir.join("details"))
        .unwrap();
    assert!(!dir.join("details/6_8923.xml").exists());
    let xml = notice(&[group("A", "1000", "HEI", ""), group("B", "2400", "ALT", "")]);
    let target = warm.due_fetch_targets(1).unwrap().remove(0);
    warm.record_success(&target, &xml).unwrap();
    warm.record_success(&old_target, &notice(&[group("LATE", "900", "ALT", "")]))
        .unwrap();
    let reopened = TfrDetailBackfillStore::new(warm.root());
    reopened
        .copy_current_details_to(&current, &dir.join("details"))
        .unwrap();
    let cold = TfrDetailBackfillStore::new(cold_root.path().join("cache"));
    cold.reconcile_desired_tfrs(&current, 1).unwrap();
    cold.record_success(&cold.due_fetch_targets(1).unwrap().remove(0), &xml)
        .unwrap();
    let cold_dir = input(cold_root.path());
    cold.copy_current_details_to(&current, &cold_dir.join("details"))
        .unwrap();
    assert_eq!(publish(&dir)["area_group_count"], 3);
    assert_eq!(publish(&dir), publish(&cold_dir));
}

#[test]
fn tfr_scoped_foreign_or_malformed_detail_cannot_replace_valid_geometry() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let baseline = publish(&dir);
    let valid = notice(&[group("A", "1000", "HEI", "")]);
    for invalid in [
        valid.replace("<noSeqNo>8923", "<noSeqNo>8927"),
        valid.replace("<geoLat>48N", "<geoLat>248N"),
        valid.replace("<AbdUid><AseUid><codeId>A", "<AbdUid><AseUid><codeId>B"),
        valid.replace(
            "</XNOTAM-Update>",
            "<Not><NotUid><noSeqNo>1234</noSeqNo></NotUid></Not></XNOTAM-Update>",
        ),
    ] {
        fs::write(dir.join("details/6_8923.xml"), invalid).unwrap();
        assert_eq!(publish(&dir), baseline);
    }
}

#[test]
fn tfr_scoped_daily_windows_include_the_previous_evening_at_midnight() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let schedule = r#"<dayCode>Daily</dayCode><ScheduleGroup><isTimeSeparate>TRUE</isTimeSeparate>
      <dateEffective>2026-10-15T00:00:00</dateEffective><dateExpire>2026-10-16T01:00:00</dateExpire>
      <startTime>2026-01-01T22:00:00</startTime><endTime>2026-01-01T02:00:00</endTime></ScheduleGroup>"#;
    fs::write(
        dir.join("details/6_8923.xml"),
        notice(&[group("A", "1000", "HEI", schedule)]),
    )
    .unwrap();
    let state = publish(&dir);
    let area = state["areas"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["notam_id"] == "6/8923")
        .unwrap();
    assert_eq!(
        area["schedule_fragments"],
        serde_json::json!([
          {"kind":"effective","value_utc":"2026-10-15T00:00:00+00:00"},
          {"kind":"expires","value_utc":"2026-10-15T02:00:00+00:00"},
          {"kind":"effective","value_utc":"2026-10-15T22:00:00+00:00"},
          {"kind":"expires","value_utc":"2026-10-16T01:00:00+00:00"}
        ])
    );
}

#[test]
fn tfr_scoped_contradictory_area_identity_isolated_to_its_notice() {
    let root = tempfile::tempdir().unwrap();
    let dir = input(root.path());
    let baseline = publish(&dir);
    let a = group("A", "1000", "HEI", "");
    let b = group("A", "2400", "ALT", "");
    fs::write(dir.join("details/6_8923.xml"), notice(&[a, b])).unwrap();
    assert_eq!(publish(&dir), baseline);
}

use std::collections::BTreeMap;
use std::io::BufRead;

use anyhow::{bail, Context};
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, Writer};
use serde::{Deserialize, Serialize};

use crate::{normalize_notam_xml, NotamNormalizationHints, StructuredNotamRecord};

const FRAGMENT_ROOT_START: &[u8] = br#"<nms-fragment
    xmlns="http://www.aixm.aero/schema/5.1/message"
    xmlns:aixm="http://www.aixm.aero/schema/5.1"
    xmlns:event="http://www.aixm.aero/schema/5.1/event"
    xmlns:fes="http://www.opengis.net/fes/2.0"
    xmlns:fns="urn:us.gov.dot.faa.aim.fns"
    xmlns:gml="http://www.opengis.net/gml/3.2"
    xmlns:gco="http://www.isotc211.org/2005/gco"
    xmlns:gmd="http://www.isotc211.org/2005/gmd"
    xmlns:gsr="http://www.isotc211.org/2005/gsr"
    xmlns:gss="http://www.isotc211.org/2005/gss"
    xmlns:gts="http://www.isotc211.org/2005/gts"
    xmlns:html="http://www.w3.org/1999/xhtml"
    xmlns:fnse="http://www.aixm.aero/schema/5.1/extensions/FAA/FNSE"
    xmlns:ows="http://www.opengis.net/ows/1.1"
    xmlns:wfs="http://www.opengis.net/wfs/2.0"
    xmlns:wfs-util="http://www.opengis.net/wfs-util/2.0"
    xmlns:xlink="http://www.w3.org/1999/xlink"
    xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#;
const FRAGMENT_ROOT_END: &[u8] = b"</nms-fragment>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum NmsNotamClassification {
    Domestic,
    Fdc,
}

impl NmsNotamClassification {
    pub fn api_name(self) -> &'static str {
        match self {
            Self::Domestic => "DOMESTIC",
            Self::Fdc => "FDC",
        }
    }

    fn source_type(self) -> &'static str {
        match self {
            Self::Domestic => "D",
            Self::Fdc => "F",
        }
    }

    fn accepts_xml_classification(self, value: &str) -> bool {
        match self {
            Self::Domestic => matches!(
                value.trim().to_ascii_uppercase().as_str(),
                "DOM" | "DOMESTIC"
            ),
            Self::Fdc => value.trim().eq_ignore_ascii_case("FDC"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NmsReferencedNotam {
    pub source_type: String,
    pub location: String,
    pub year: String,
    pub notam_type: String,
    pub number: String,
}

impl NmsReferencedNotam {
    pub fn human_identity(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            self.source_type, self.location, self.year, self.notam_type, self.number
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NmsApiUpdateAction {
    Upsert,
    RemoveSelf,
    RemoveReferenced,
}

#[derive(Debug, Clone)]
pub struct NmsApiUpdate {
    pub action: NmsApiUpdateAction,
    pub record: StructuredNotamRecord,
    pub referenced_notam: Option<NmsReferencedNotam>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NmsInitialLoadRejection {
    pub ordinal: usize,
    pub nms_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmsInitialLoadParseResult {
    pub classification: NmsNotamClassification,
    pub feature_collection_timestamp: Option<String>,
    pub declared_record_count: Option<usize>,
    pub parsed_message_count: usize,
    pub duplicate_record_ids: Vec<String>,
    pub records: Vec<StructuredNotamRecord>,
    pub rejections: Vec<NmsInitialLoadRejection>,
}

impl NmsInitialLoadParseResult {
    pub fn validate_complete(&self) -> anyhow::Result<()> {
        if let Some(declared) = self.declared_record_count {
            if declared != self.parsed_message_count {
                bail!(
                    "{} Initial Load declared {declared} records but contained {} messages",
                    self.classification.api_name(),
                    self.parsed_message_count
                );
            }
        }
        if !self.rejections.is_empty() {
            bail!(
                "{} Initial Load rejected {} of {} messages",
                self.classification.api_name(),
                self.rejections.len(),
                self.parsed_message_count
            );
        }
        if !self.duplicate_record_ids.is_empty() {
            bail!(
                "{} Initial Load contains {} duplicate canonical IDs",
                self.classification.api_name(),
                self.duplicate_record_ids.len()
            );
        }
        Ok(())
    }
}

pub fn parse_nms_initial_load<R: BufRead>(
    input: R,
    classification: NmsNotamClassification,
) -> anyhow::Result<NmsInitialLoadParseResult> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut feature_collection_timestamp = None;
    let mut declared_record_count = None;
    let mut capture: Option<(Writer<Vec<u8>>, usize)> = None;
    let mut parsed_message_count = 0usize;
    let mut records_by_id = BTreeMap::new();
    let mut duplicate_record_ids = Vec::new();
    let mut rejections = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .context("failed to read NMS Initial Load XML")?;

        if let Event::Start(start) = &event {
            if start.local_name().as_ref() == b"FeatureCollection" {
                feature_collection_timestamp = attribute_value(&reader, start, b"timeStamp")?;
                declared_record_count = attribute_value(&reader, start, b"numberReturned")?
                    .map(|value| value.parse::<usize>())
                    .transpose()
                    .context("NMS Initial Load has invalid numberReturned")?;
            }
        }

        if capture.is_none() {
            if let Event::Start(start) = &event {
                if start.local_name().as_ref() == b"AIXMBasicMessage" {
                    let mut writer = Writer::new(Vec::new());
                    writer.write_event(event.clone().into_owned())?;
                    capture = Some((writer, 1));
                }
            }
        } else {
            let (writer, depth) = capture.as_mut().expect("capture exists");
            match &event {
                Event::Start(_) => {
                    *depth += 1;
                    writer.write_event(event.clone().into_owned())?;
                }
                Event::End(_) => {
                    writer.write_event(event.clone().into_owned())?;
                    *depth -= 1;
                    if *depth == 0 {
                        let (writer, _) = capture.take().expect("capture exists");
                        parsed_message_count += 1;
                        let fragment = wrap_fragment(writer.into_inner());
                        match normalize_initial_load_fragment(&fragment, classification) {
                            Ok(record) => {
                                let record_id = record.id.clone();
                                if records_by_id.insert(record_id.clone(), record).is_some() {
                                    duplicate_record_ids.push(record_id);
                                }
                            }
                            Err(error) => rejections.push(NmsInitialLoadRejection {
                                ordinal: parsed_message_count,
                                nms_id: nms_id_from_fragment(&fragment),
                                reason: format!("{error:#}"),
                            }),
                        }
                    }
                }
                Event::Eof => bail!("NMS Initial Load ended inside AIXMBasicMessage"),
                _ => writer.write_event(event.clone().into_owned())?,
            }
        }

        if matches!(event, Event::Eof) {
            break;
        }
        buffer.clear();
    }

    duplicate_record_ids.sort();
    duplicate_record_ids.dedup();
    Ok(NmsInitialLoadParseResult {
        classification,
        feature_collection_timestamp,
        declared_record_count,
        parsed_message_count,
        duplicate_record_ids,
        records: records_by_id.into_values().collect(),
        rejections,
    })
}

fn normalize_initial_load_fragment(
    fragment: &str,
    classification: NmsNotamClassification,
) -> anyhow::Result<StructuredNotamRecord> {
    let document = roxmltree::Document::parse(fragment)
        .context("failed to parse NMS AIXMBasicMessage fragment")?;
    let xml_classification = document
        .descendants()
        .find(|node| node.tag_name().name() == "classification")
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(xml_classification) = xml_classification {
        if !classification.accepts_xml_classification(xml_classification) {
            bail!(
                "{} endpoint returned {xml_classification} record",
                classification.api_name()
            );
        }
    }

    normalize_notam_xml(
        fragment,
        NotamNormalizationHints {
            source_type: Some(classification.source_type().to_string()),
            notam_status: Some("ACTIVE".to_string()),
            ..NotamNormalizationHints::default()
        },
    )?
    .context("NMS AIXMBasicMessage contains no NOTAM")
}

pub fn parse_nms_api_update(
    xml: &str,
    classification: NmsNotamClassification,
) -> anyhow::Result<NmsApiUpdate> {
    let document =
        roxmltree::Document::parse(xml).context("failed to parse NMS API AIXM update")?;
    validate_classification(&document, classification)?;
    let notam = document
        .descendants()
        .find(|node| node.tag_name().name() == "NOTAM")
        .context("NMS API AIXM update contains no NOTAM")?;
    let raw_type = child_text(&notam, "type").map(|value| value.to_ascii_uppercase());
    let canceled = first_text(&document, "canceled");
    let action = if canceled.is_some() {
        NmsApiUpdateAction::RemoveSelf
    } else if raw_type.as_deref() == Some("C") {
        NmsApiUpdateAction::RemoveReferenced
    } else {
        NmsApiUpdateAction::Upsert
    };
    let status = match action {
        NmsApiUpdateAction::Upsert => "ACTIVE",
        NmsApiUpdateAction::RemoveSelf | NmsApiUpdateAction::RemoveReferenced => "CANCELLED",
    };
    let inferred_type = raw_type.clone().unwrap_or_else(|| "N".to_string());
    let inferred_function = match action {
        NmsApiUpdateAction::Upsert => inferred_type.clone(),
        NmsApiUpdateAction::RemoveSelf | NmsApiUpdateAction::RemoveReferenced => "C".to_string(),
    };
    let record = normalize_notam_xml(
        xml,
        NotamNormalizationHints {
            source_type: Some(classification.source_type().to_string()),
            notam_status: Some(status.to_string()),
            notam_function: Some(inferred_function),
            notam_type: Some(inferred_type),
            ..NotamNormalizationHints::default()
        },
    )?
    .context("NMS API AIXM update contains no canonical NOTAM")?;
    let referenced_notam = match action {
        NmsApiUpdateAction::RemoveReferenced => Some(referenced_notam(&record, classification)?),
        NmsApiUpdateAction::Upsert if raw_type.as_deref() == Some("R") => {
            referenced_notam(&record, classification).ok()
        }
        NmsApiUpdateAction::Upsert | NmsApiUpdateAction::RemoveSelf => None,
    };
    Ok(NmsApiUpdate {
        action,
        record,
        referenced_notam,
    })
}

fn validate_classification(
    document: &roxmltree::Document<'_>,
    classification: NmsNotamClassification,
) -> anyhow::Result<()> {
    let xml_classification = first_text(document, "classification");
    if let Some(xml_classification) = xml_classification {
        if !classification.accepts_xml_classification(&xml_classification) {
            bail!(
                "{} endpoint returned {xml_classification} record",
                classification.api_name()
            );
        }
    }
    Ok(())
}

fn referenced_notam(
    record: &StructuredNotamRecord,
    classification: NmsNotamClassification,
) -> anyhow::Result<NmsReferencedNotam> {
    let local_text = record.local_text.as_deref().unwrap_or_default();
    let icao_text = record.icao_text.as_deref().unwrap_or_default();
    let marker = if record.notam_function.as_deref() == Some("NOTAMR") {
        "NOTAMR"
    } else {
        "NOTAMC"
    };
    let number = token_after_marker(icao_text, marker)
        .or_else(|| token_after_marker(local_text, marker))
        .or_else(|| token_after_marker(local_text, "CANCEL"))
        .map(normalize_reference_token)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("NMS API {marker} update has no referenced NOTAM number"))?;
    let location = location_after_a_marker(icao_text)
        .or_else(|| {
            if classification == NmsNotamClassification::Fdc {
                token_after_marker(local_text, "CANCEL")
                    .and_then(|_| local_text.split_whitespace().last())
                    .map(normalize_reference_token)
            } else {
                None
            }
        })
        .or_else(|| record.location.clone())
        .filter(|value| !value.is_empty())
        .context("NMS API cancellation has no referenced NOTAM location")?;
    let year = referenced_year(&number, record.notam_year.as_deref())?;
    let number = canonical_referenced_number(&number, classification)?;
    Ok(NmsReferencedNotam {
        source_type: classification.source_type().to_string(),
        location: location.to_ascii_uppercase(),
        year,
        notam_type: "N".to_string(),
        number,
    })
}

fn token_after_marker<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let mut tokens = text.split_whitespace();
    while let Some(token) = tokens.next() {
        if normalize_reference_token(token).eq_ignore_ascii_case(marker) {
            return tokens.next();
        }
    }
    None
}

fn location_after_a_marker(text: &str) -> Option<String> {
    let mut tokens = text.split_whitespace();
    while let Some(token) = tokens.next() {
        if normalize_reference_token(token).eq_ignore_ascii_case("A") {
            return tokens
                .next()
                .map(normalize_reference_token)
                .filter(|value| !value.is_empty());
        }
    }
    None
}

fn normalize_reference_token(token: &str) -> String {
    token
        .trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '/')
        .to_ascii_uppercase()
}

fn referenced_year(number: &str, fallback: Option<&str>) -> anyhow::Result<String> {
    if let Some((_, suffix)) = number.rsplit_once('/') {
        if suffix.len() == 2 && suffix.chars().all(|character| character.is_ascii_digit()) {
            let century = fallback.and_then(|year| year.get(..2)).unwrap_or("20");
            return Ok(format!("{century}{suffix}"));
        }
    }
    fallback
        .map(str::to_string)
        .context("NMS API referenced NOTAM has no year")
}

fn canonical_referenced_number(
    number: &str,
    classification: NmsNotamClassification,
) -> anyhow::Result<String> {
    if classification == NmsNotamClassification::Fdc {
        let (series, sequence) = number
            .split_once('/')
            .context("FDC referenced NOTAM number has no slash")?;
        if !series.chars().all(|character| character.is_ascii_digit())
            || !sequence.chars().all(|character| character.is_ascii_digit())
        {
            bail!("invalid FDC referenced NOTAM number {number}");
        }
        return Ok(format!(
            "{}/{:04}",
            series.parse::<u64>()?,
            sequence.parse::<u64>()?
        ));
    }
    let (series, sequence) = number
        .split_once('/')
        .context("Domestic referenced NOTAM number has no slash")?;
    if !series.chars().all(|character| character.is_ascii_digit())
        || !sequence.chars().all(|character| character.is_ascii_digit())
    {
        bail!("invalid Domestic referenced NOTAM number {number}");
    }
    Ok(format!(
        "{:02}/{:03}",
        series.parse::<u64>()?,
        sequence.parse::<u64>()?
    ))
}

fn first_text(document: &roxmltree::Document<'_>, name: &str) -> Option<String> {
    document
        .descendants()
        .find(|node| node.tag_name().name() == name)
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn child_text(node: &roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn attribute_value<R: BufRead>(
    reader: &Reader<R>,
    start: &BytesStart<'_>,
    name: &[u8],
) -> anyhow::Result<Option<String>> {
    for attribute in start.attributes() {
        let attribute = attribute.context("NMS Initial Load has malformed XML attribute")?;
        if attribute.key.local_name().as_ref() == name {
            return Ok(Some(
                attribute
                    .decode_and_unescape_value(reader.decoder())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn wrap_fragment(fragment: Vec<u8>) -> String {
    let mut wrapped =
        Vec::with_capacity(FRAGMENT_ROOT_START.len() + fragment.len() + FRAGMENT_ROOT_END.len());
    wrapped.extend_from_slice(FRAGMENT_ROOT_START);
    wrapped.extend_from_slice(&fragment);
    wrapped.extend_from_slice(FRAGMENT_ROOT_END);
    String::from_utf8(wrapped).expect("quick-xml emitted non-UTF-8 XML")
}

fn nms_id_from_fragment(fragment: &str) -> Option<String> {
    let document = roxmltree::Document::parse(fragment).ok()?;
    document
        .descendants()
        .find(|node| node.tag_name().name() == "AIXMBasicMessage")?
        .attributes()
        .find(|attribute| attribute.name() == "id")
        .map(|attribute| attribute.value().trim())
        .and_then(|value| value.strip_prefix("NMS_ID_").or(Some(value)))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/nms_initial_load.xml");

    #[test]
    fn parses_initial_load_into_canonical_records() -> anyhow::Result<()> {
        let parsed = parse_nms_initial_load(
            Cursor::new(FIXTURE.as_bytes()),
            NmsNotamClassification::Domestic,
        )?;

        assert_eq!(
            parsed.feature_collection_timestamp.as_deref(),
            Some("2025-09-12T17:24:02.017Z")
        );
        assert_eq!(parsed.declared_record_count, Some(1));
        assert_eq!(parsed.parsed_message_count, 1);
        assert!(parsed.rejections.is_empty());
        assert!(parsed.duplicate_record_ids.is_empty());
        parsed.validate_complete()?;

        let record = &parsed.records[0];
        assert_eq!(
            record.local_text.as_deref(),
            Some("!STL 08/430 8WC RWY 20 RWY END ID LGT U/S 2508210234-2510012359")
        );
        assert_eq!(record.id, "NMS:1757609538792382");
        assert_eq!(record.nms_id.as_deref(), Some("1757609538792382"));
        assert_eq!(
            record.last_updated_utc.as_deref(),
            Some("2025-08-21T02:34:00.000Z")
        );
        assert_eq!(record.notam_status.as_deref(), Some("ACTIVE"));
        assert_eq!(record.notam_function.as_deref(), Some("NOTAMN"));
        Ok(())
    }

    #[test]
    fn rejects_a_classification_mismatch() -> anyhow::Result<()> {
        let parsed =
            parse_nms_initial_load(Cursor::new(FIXTURE.as_bytes()), NmsNotamClassification::Fdc)?;
        assert_eq!(parsed.rejections.len(), 1);
        assert!(parsed.rejections[0]
            .reason
            .contains("FDC endpoint returned DOM record"));
        Ok(())
    }

    #[test]
    fn parses_same_id_cancellation_without_notam_type() -> anyhow::Result<()> {
        let update = parse_nms_api_update(
            &update_xml(
                "1784815703556165",
                "DOM",
                "OME",
                "103",
                "",
                "!OME 07/103 OME TWY ALL FICON WET OBS AT 2607231405.",
                "<fnse:canceled>2026-07-23T14:16:00.000Z</fnse:canceled>",
            ),
            NmsNotamClassification::Domestic,
        )?;

        assert_eq!(update.action, NmsApiUpdateAction::RemoveSelf);
        assert_eq!(update.record.id, "NMS:1784815703556165");
        assert_eq!(update.record.notam_type.as_deref(), Some("N"));
        assert_eq!(update.record.notam_status.as_deref(), Some("CANCELLED"));
        assert_eq!(update.record.notam_number.as_deref(), Some("07/103"));
        assert!(update.referenced_notam.is_none());
        Ok(())
    }

    #[test]
    fn parses_fdc_cancellation_reference() -> anyhow::Result<()> {
        let update = parse_nms_api_update(
            &update_xml(
                "1784816955747870",
                "FDC",
                "FDC",
                "7893",
                "<event:type>C</event:type>",
                "!FDC 6/7893 FDC CANCEL 6/7893 MHK",
                "",
            )
            .replace(
                "<event:text>TEST</event:text>",
                "<event:text>FDC 6/7893 NOTAMC 6/7893\nA) MHK</event:text>",
            ),
            NmsNotamClassification::Fdc,
        )?;

        assert_eq!(update.action, NmsApiUpdateAction::RemoveReferenced);
        assert_eq!(
            update
                .referenced_notam
                .as_ref()
                .map(NmsReferencedNotam::human_identity)
                .as_deref(),
            Some("F:MHK:2026:N:6/7893")
        );
        Ok(())
    }

    fn update_xml(
        nms_id: &str,
        classification: &str,
        location: &str,
        number: &str,
        notam_type: &str,
        local_text: &str,
        extension: &str,
    ) -> String {
        format!(
            r#"<AIXMBasicMessage xmlns="http://www.aixm.aero/schema/5.1/message"
                xmlns:event="http://www.aixm.aero/schema/5.1/event"
                xmlns:gml="http://www.opengis.net/gml/3.2"
                xmlns:fnse="http://www.aixm.aero/schema/5.1/extensions/FAA/FNSE"
                gml:id="NMS_ID_{nms_id}">
              <hasMember><event:Event><event:timeSlice><event:EventTimeSlice>
                <event:scenario>110</event:scenario>
                <event:textNOTAM><event:NOTAM>
                  <event:number>{number}</event:number>
                  <event:year>2026</event:year>
                  {notam_type}
                  <event:issued>2026-07-23T14:07:00.000Z</event:issued>
                  <event:location>{location}</event:location>
                  <event:effectiveStart>202607231405</event:effectiveStart>
                  <event:effectiveEnd>202607241405</event:effectiveEnd>
                  <event:text>TEST</event:text>
                  <event:translation><event:NOTAMTranslation>
                    <event:type>LOCAL_FORMAT</event:type>
                    <event:simpleText>{local_text}</event:simpleText>
                  </event:NOTAMTranslation></event:translation>
                </event:NOTAM></event:textNOTAM>
                <event:extension><fnse:EventExtension>
                  <fnse:classification>{classification}</fnse:classification>
                  <fnse:lastUpdated>2026-07-23T14:16:00.000Z</fnse:lastUpdated>
                  {extension}
                </fnse:EventExtension></event:extension>
              </event:EventTimeSlice></event:timeSlice></event:Event></hasMember>
            </AIXMBasicMessage>"#
        )
    }
}

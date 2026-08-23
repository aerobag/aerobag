// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

use app_ui_contracts::session::{
    UiAircraftLibraryAction, UiAircraftLibraryEditor, UiAircraftLibraryEntry,
    UiAircraftLibraryState, UiAircraftSymbol, UiSettingsSyncIndicator,
};

use crate::{AppError, AppErrorKind, AppResult};

const ADD_ACTION_ID: &str = "aircraft_library/add";
const SAVE_ACTION_ID: &str = "aircraft_library/save";
const CANCEL_ACTION_ID: &str = "aircraft_library/cancel";
const EDIT_ACTION_PREFIX: &str = "aircraft_library/edit/";
const INCLUDE_ACTION_PREFIX: &str = "aircraft_library/include/";
const EXCLUDE_ACTION_PREFIX: &str = "aircraft_library/exclude/";
pub(crate) const SUPERSEDED_IMPORT_ERROR: &str = "This aircraft definition is superseded by another definition in your library. Change any part of it to make it a fresh definition.";
pub(crate) const AIRCRAFT_SYMBOL_ROTATION_DEGREES: i16 = -45;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AircraftLibraryAction {
    BeginAdd,
    BeginEdit {
        definition_hash: String,
    },
    SetIncluded {
        definition_hash: String,
        included: bool,
    },
    Save,
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AircraftLibraryEditorModel {
    pub replacing_definition_hash: Option<String>,
    pub expected_lineage_id: Option<String>,
    pub source_json: String,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AircraftCatalog {
    pub definitions: BTreeMap<String, product_contracts::AircraftDefinition>,
    pub system_hashes: BTreeSet<String>,
    pub included_hashes: BTreeSet<String>,
    superseded_hashes: BTreeSet<String>,
}

pub(crate) fn parse_action(action_id: &str) -> AppResult<AircraftLibraryAction> {
    let action = match action_id {
        ADD_ACTION_ID => AircraftLibraryAction::BeginAdd,
        SAVE_ACTION_ID => AircraftLibraryAction::Save,
        CANCEL_ACTION_ID => AircraftLibraryAction::Cancel,
        _ => {
            if let Some(hash) = action_id.strip_prefix(EDIT_ACTION_PREFIX) {
                validate_hash(hash)?;
                AircraftLibraryAction::BeginEdit {
                    definition_hash: hash.to_string(),
                }
            } else if let Some(hash) = action_id.strip_prefix(INCLUDE_ACTION_PREFIX) {
                validate_hash(hash)?;
                AircraftLibraryAction::SetIncluded {
                    definition_hash: hash.to_string(),
                    included: true,
                }
            } else if let Some(hash) = action_id.strip_prefix(EXCLUDE_ACTION_PREFIX) {
                validate_hash(hash)?;
                AircraftLibraryAction::SetIncluded {
                    definition_hash: hash.to_string(),
                    included: false,
                }
            } else {
                return Err(invalid_action(action_id));
            }
        }
    };
    Ok(action)
}

pub(crate) fn new_editor() -> AppResult<AircraftLibraryEditorModel> {
    let mut definition: product_contracts::AircraftDefinition = serde_json::from_str(include_str!(
        "../../../../../product/preprocessor/preprocessor-cli/resources/aircraft/cessna-172-generic.json"
    ))
    .map_err(|error| internal_error(format!("invalid aircraft editor template: {error}")))?;
    definition.lineage_id = "my-aircraft".to_string();
    definition.manufacturer = "Manufacturer".to_string();
    definition.model = "Model".to_string();
    definition.label = "MY AIRCRAFT".to_string();
    definition.profiles[0].source =
        "Replace with the source and assumptions for this aircraft".to_string();
    definition.supersedes.clear();
    Ok(AircraftLibraryEditorModel {
        replacing_definition_hash: None,
        expected_lineage_id: None,
        source_json: pretty_definition(&definition)?,
        validation_error: None,
    })
}

pub(crate) fn edit_definition(
    definition_hash: &str,
    definition: &product_contracts::AircraftDefinition,
) -> AppResult<AircraftLibraryEditorModel> {
    validate_hash(definition_hash)?;
    Ok(AircraftLibraryEditorModel {
        replacing_definition_hash: Some(definition_hash.to_string()),
        expected_lineage_id: Some(definition.lineage_id.clone()),
        source_json: pretty_definition(definition)?,
        validation_error: None,
    })
}

pub(crate) fn validate_source(
    source_json: &str,
    editor: &AircraftLibraryEditorModel,
) -> Result<(String, product_contracts::AircraftDefinition, String), String> {
    if source_json.len() > product_contracts::MAX_AIRCRAFT_DEFINITION_JSON_BYTES {
        return Err(format!(
            "Aircraft definition exceeds {} bytes",
            product_contracts::MAX_AIRCRAFT_DEFINITION_JSON_BYTES
        ));
    }
    let mut definition: product_contracts::AircraftDefinition =
        serde_json::from_str(source_json).map_err(json_error_message)?;
    if let Some(expected) = editor.expected_lineage_id.as_deref() {
        if definition.lineage_id != expected {
            return Err(format!(
                "Aircraft lineage_id must remain {expected:?} when editing this model"
            ));
        }
    }
    if let Some(replaced) = editor.replacing_definition_hash.as_ref() {
        if !definition.supersedes.contains(replaced) {
            definition.supersedes.push(replaced.clone());
            definition.supersedes.sort();
        }
    }
    definition
        .validate()
        .map_err(|error| format!("Invalid aircraft model: {error}"))?;
    let hash = definition
        .content_hash()
        .map_err(|error| format!("Invalid aircraft model: {error}"))?;
    for profile in &definition.profiles {
        crate::performance_profile_from_definition(&hash, &definition, &profile.id).map_err(
            |error| {
                format!(
                    "Aircraft profile {:?} cannot be used by the planner: {error}",
                    profile.id
                )
            },
        )?;
    }
    let normalized = pretty_definition(&definition).map_err(|error| error.message)?;
    Ok((hash, definition, normalized))
}

pub(crate) fn build_catalog(
    system_definitions: BTreeMap<String, product_contracts::AircraftDefinition>,
    private_definitions: &BTreeMap<String, product_contracts::AircraftDefinition>,
    memberships: &BTreeMap<String, product_contracts::AircraftLibraryMembership>,
) -> AircraftCatalog {
    let superseded_hashes = superseded_hashes(
        system_definitions
            .values()
            .chain(private_definitions.values()),
    );
    let system_hashes = system_definitions.keys().cloned().collect::<BTreeSet<_>>();
    let mut definitions = system_definitions;
    for (hash, definition) in private_definitions {
        definitions
            .entry(hash.clone())
            .or_insert_with(|| definition.clone());
    }
    let included_hashes = definitions
        .keys()
        .filter(|hash| {
            memberships
                .get(*hash)
                .map(|membership| membership.included)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    AircraftCatalog {
        definitions,
        system_hashes,
        included_hashes,
        superseded_hashes,
    }
}

pub(crate) fn definition_is_superseded<'a>(
    definition_hash: &str,
    definitions: impl IntoIterator<Item = &'a product_contracts::AircraftDefinition>,
) -> bool {
    definitions.into_iter().any(|definition| {
        definition
            .supersedes
            .iter()
            .any(|hash| hash == definition_hash)
    })
}

fn superseded_hashes<'a>(
    definitions: impl IntoIterator<Item = &'a product_contracts::AircraftDefinition>,
) -> BTreeSet<String> {
    definitions
        .into_iter()
        .flat_map(|definition| definition.supersedes.iter().cloned())
        .collect()
}

pub(crate) fn project_state(
    catalog: &AircraftCatalog,
    editor: Option<&AircraftLibraryEditorModel>,
    sync_indicator: Option<UiSettingsSyncIndicator>,
) -> UiAircraftLibraryState {
    let mut entries = catalog
        .definitions
        .iter()
        .filter(|(hash, _)| !catalog.superseded_hashes.contains(*hash))
        .map(|(hash, definition)| {
            let included = catalog.included_hashes.contains(hash);
            let is_system = catalog.system_hashes.contains(hash);
            UiAircraftLibraryEntry {
                definition_hash: hash.clone(),
                label: definition.label.clone(),
                source_label: if is_system { "SYSTEM" } else { "USER" }.to_string(),
                included,
                symbol: aircraft_symbol(definition),
                toggle_action: action(
                    if included {
                        format!("{EXCLUDE_ACTION_PREFIX}{hash}")
                    } else {
                        format!("{INCLUDE_ACTION_PREFIX}{hash}")
                    },
                    if included { "Hide" } else { "Show" },
                ),
                edit_action: (!is_system)
                    .then(|| action(format!("{EDIT_ACTION_PREFIX}{hash}"), "Edit")),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_is_system = left.source_label == "SYSTEM";
        let right_is_system = right.source_label == "SYSTEM";
        left_is_system
            .cmp(&right_is_system)
            .then_with(|| left.label.to_uppercase().cmp(&right.label.to_uppercase()))
            .then_with(|| left.definition_hash.cmp(&right.definition_hash))
    });
    UiAircraftLibraryState {
        title: "Aircraft library".to_string(),
        summary:
            "Choose aircraft for the Altitude page short menu, or add a private performance model."
                .to_string(),
        sync_indicator,
        entries,
        add_action: action(ADD_ACTION_ID.to_string(), "+ Add aircraft"),
        editor: editor.map(|editor| UiAircraftLibraryEditor {
            title: if editor.replacing_definition_hash.is_some() {
                "Edit user aircraft"
            } else {
                "Add user aircraft"
            }
            .to_string(),
            field_label: "Aircraft definition JSON".to_string(),
            source_json: editor.source_json.clone(),
            validation_error: editor.validation_error.clone(),
            save_action: action(SAVE_ACTION_ID.to_string(), "Save aircraft"),
            cancel_action: action(CANCEL_ACTION_ID.to_string(), "Cancel"),
        }),
    }
}

pub(crate) fn aircraft_symbol(
    definition: &product_contracts::AircraftDefinition,
) -> UiAircraftSymbol {
    UiAircraftSymbol {
        path_data: definition.plan_view_path.clone(),
        rotation_degrees: AIRCRAFT_SYMBOL_ROTATION_DEGREES,
    }
}

fn pretty_definition(definition: &product_contracts::AircraftDefinition) -> AppResult<String> {
    serde_json::to_string_pretty(definition)
        .map_err(|error| internal_error(format!("failed to encode aircraft definition: {error}")))
}

fn json_error_message(error: serde_json::Error) -> String {
    let prefix = match error.classify() {
        serde_json::error::Category::Syntax | serde_json::error::Category::Eof => "Invalid JSON",
        serde_json::error::Category::Data => "Invalid aircraft definition",
        serde_json::error::Category::Io => "Cannot read aircraft definition",
    };
    format!(
        "{prefix} at line {}, column {}: {error}",
        error.line(),
        error.column()
    )
}

fn action(action_id: String, label: &str) -> UiAircraftLibraryAction {
    UiAircraftLibraryAction {
        action_id,
        label: label.to_string(),
        enabled: true,
        disabled_reason: None,
    }
}

fn validate_hash(hash: &str) -> AppResult<()> {
    product_contracts::validate_aircraft_definition_hash(hash).map_err(|_| invalid_action(hash))
}

fn invalid_action(action_id: &str) -> AppError {
    AppError {
        kind: AppErrorKind::UnsupportedOperation,
        message: format!("unsupported aircraft-library action {action_id:?}"),
    }
}

fn internal_error(message: String) -> AppError {
    AppError {
        kind: AppErrorKind::Internal,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template_editor() -> AircraftLibraryEditorModel {
        new_editor().expect("editor template")
    }

    fn definition_with_label(label: &str) -> (String, product_contracts::AircraftDefinition) {
        let editor = template_editor();
        let (_, mut definition, _) =
            validate_source(&editor.source_json, &editor).expect("valid template");
        definition.label = label.to_string();
        definition.lineage_id = label.to_ascii_lowercase().replace(' ', "-");
        let hash = definition.content_hash().expect("content hash");
        (hash, definition)
    }

    #[test]
    fn validation_runs_the_complete_definition_and_planner_path() {
        let editor = template_editor();
        let (hash, definition, normalized) =
            validate_source(&editor.source_json, &editor).expect("valid template");
        assert_eq!(hash, definition.content_hash().expect("content hash"));
        assert_eq!(
            serde_json::from_str::<product_contracts::AircraftDefinition>(&normalized)
                .expect("normalized definition"),
            definition
        );
        assert!(definition.profiles.iter().all(|profile| {
            crate::performance_profile_from_definition(&hash, &definition, &profile.id).is_ok()
        }));
    }

    #[test]
    fn validation_reports_bad_icons_without_persisting_a_partial_model() {
        let editor = template_editor();
        let mut value: serde_json::Value =
            serde_json::from_str(&editor.source_json).expect("template JSON");
        value["plan_view_path"] = serde_json::Value::String("M 0 0 Q 1 2 3 4".to_string());
        let error = validate_source(&value.to_string(), &editor).expect_err("invalid path");
        assert!(error.contains("plan_view_path"), "{error}");
    }

    #[test]
    fn editing_preserves_lineage_and_supersedes_the_replaced_definition() {
        let initial = template_editor();
        let (old_hash, old_definition, _) =
            validate_source(&initial.source_json, &initial).expect("initial model");
        let editor = edit_definition(&old_hash, &old_definition).expect("edit model");
        let mut edited = old_definition.clone();
        edited.label = "MY AIRCRAFT REVISED".to_string();
        let (new_hash, definition, _) = validate_source(
            &serde_json::to_string(&edited).expect("edited JSON"),
            &editor,
        )
        .expect("revised model");
        assert_ne!(new_hash, old_hash);
        assert!(definition.supersedes.contains(&old_hash));

        edited.lineage_id = "different-lineage".to_string();
        assert!(validate_source(
            &serde_json::to_string(&edited).expect("wrong-lineage JSON"),
            &editor,
        )
        .expect_err("lineage mutation")
        .contains("lineage_id must remain"));
    }

    #[test]
    fn private_aircraft_sort_before_system_aircraft_then_by_label() {
        let (system_beta_hash, system_beta) = definition_with_label("BETA SYSTEM");
        let (system_alpha_hash, system_alpha) = definition_with_label("ALPHA SYSTEM");
        let (private_zulu_hash, private_zulu) = definition_with_label("ZULU USER");
        let (private_alpha_hash, private_alpha) = definition_with_label("ALPHA USER");
        let catalog = build_catalog(
            BTreeMap::from([
                (system_beta_hash, system_beta),
                (system_alpha_hash, system_alpha),
            ]),
            &BTreeMap::from([
                (private_zulu_hash, private_zulu),
                (private_alpha_hash, private_alpha),
            ]),
            &BTreeMap::new(),
        );

        let state = project_state(&catalog, None, None);
        assert_eq!(
            state
                .entries
                .iter()
                .map(|entry| (entry.source_label.as_str(), entry.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("USER", "ALPHA USER"),
                ("USER", "ZULU USER"),
                ("SYSTEM", "ALPHA SYSTEM"),
                ("SYSTEM", "BETA SYSTEM"),
            ]
        );
    }

    #[test]
    fn user_and_system_successors_both_make_an_import_hash_stale() {
        let (old_hash, old_definition) = definition_with_label("OLD USER");
        let (user_hash, mut user_successor) = definition_with_label("NEW USER");
        user_successor.supersedes.push(old_hash.clone());
        let (system_old_hash, system_old) = definition_with_label("OLD SYSTEM");
        let (_, mut system_successor) = definition_with_label("NEW SYSTEM");
        system_successor.supersedes.push(system_old_hash.clone());

        assert!(definition_is_superseded(
            &old_hash,
            [&old_definition, &user_successor],
        ));
        assert!(definition_is_superseded(
            &system_old_hash,
            [&system_old, &system_successor],
        ));
        assert!(!definition_is_superseded(
            &user_hash,
            [
                &old_definition,
                &user_successor,
                &system_old,
                &system_successor
            ],
        ));
    }
}

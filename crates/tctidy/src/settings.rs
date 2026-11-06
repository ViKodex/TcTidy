//! What a settings UI needs: the catalog of every setting with its current
//! value, a small before/after preview for each one, and a function block to
//! format with all of them.
//!
//! The settings themselves are described once, in [`tctidy_format::options`].
//! This module adds a short ST sample per setting, chosen so that changing the
//! setting visibly changes the sample, and formats it for a given style.

use serde::{Deserialize, Serialize};
use tctidy_format::options::{style_options, OptionGroup, OptionKind, OptionValue, StyleOption};
use tctidy_format::style::BlockTokenPlacement;
use tctidy_format::StyleConfig;

use crate::{format_st, FormatOptions, StSection};

/// The ST snippet that illustrates one setting.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    /// The grammar the snippet is written in.
    pub section: StSection,
    pub source: &'static str,
    /// Line width the preview is rendered at, so that wrapping settings show
    /// on a short snippet. `None` keeps the style's own width.
    pub line_width: Option<u16>,
    /// Other settings the preview needs (e.g. `indent_block_token` only shows
    /// once `THEN` is on its own line).
    pub prepare: Option<fn(&mut StyleConfig)>,
}

const fn decl(source: &'static str) -> Sample {
    Sample {
        section: StSection::Declaration,
        source,
        line_width: None,
        prepare: None,
    }
}

const fn imp(source: &'static str) -> Sample {
    Sample {
        section: StSection::Implementation,
        source,
        line_width: None,
        prepare: None,
    }
}

const fn narrow(sample: Sample, width: u16) -> Sample {
    Sample {
        line_width: Some(width),
        ..sample
    }
}

const IF_BLOCK: &str = "IF bStart AND NOT bBusy THEN\nnCount := nCount + 1;\nEND_IF";
const LONG_CONDITION: &str =
    "bReady := bEnabled AND ((bDoorClosed AND bGuardOk) OR (bManualMode AND bKeySwitch));";
/// 131 columns: wraps at the common narrower widths (120, 100, 80), fits at the
/// default 160.
const LONG_CALL: &str =
    "MoveAbsolute(fTargetPosition, fMaxVelocity, fAcceleration, fDeceleration, \
                         fJerkLimit, nBufferMode, bExecute, bDone, bBusy, bError);";
const PARENS: &str = "fResult := fScale * (fRaw - fOffset) / (fMax - fMin);";
const DECLS: &str = "VAR\nbStart : BOOL;\nnCycleCount : INT := 0;\nfSpeed : LREAL;\nEND_VAR";

/// The sample for setting `key`, or `None` for an unknown key.
pub fn sample(key: &str) -> Option<Sample> {
    Some(match key {
        "indent_style" | "indent_width" => imp(IF_BLOCK),
        "line_width" => imp(LONG_CALL),
        "keyword_case" => imp(IF_BLOCK),
        "type_keyword_case" => decl(DECLS),
        "space_around_binary_op" => imp("nTotal := nCount * 2 + nOffset;"),
        "space_around_assign" => decl("VAR\nnCount : INT := 0;\nEND_VAR"),
        "space_after_comma" => imp("nMax := MAX(nA, nB, nC);"),
        "space_inside_parens" => imp(PARENS),
        "align_impl_assignments" => imp("nCount := 0;\nbDone := FALSE;\nfTargetPosition := 10.0;"),
        "space_before_colon" | "space_after_colon" => decl(DECLS),
        "then_placement" => imp(IF_BLOCK),
        "do_placement" => imp("FOR i := 1 TO 10 DO\naValues[i] := 0;\nEND_FOR"),
        "of_placement" => imp("CASE nState OF\n0: nState := 10;\n10: bDone := TRUE;\nEND_CASE"),
        "indent_block_token" => Sample {
            prepare: Some(|s| s.then_placement = BlockTokenPlacement::OwnLine),
            ..imp(IF_BLOCK)
        },
        "place_simple_if_on_single_line" => imp(IF_BLOCK),
        "wrap_before_operator" => narrow(
            imp("fTotal := fFirstValue + fSecondValue + fThirdValue + fFourthValue;"),
            40,
        ),
        "logical_expression_layout" => narrow(imp(LONG_CONDITION), 60),
        "logical_chain_max_inline" => {
            imp("IF bPowerOk AND bAirOk AND bDoorClosed AND NOT bEStop THEN
bReady := TRUE;
END_IF")
        }
        "parenthesized_expression_layout" => narrow(
            imp("fResult := fGain * (fFirstValue + fSecondValue + fThirdValue);"),
            40,
        ),
        "call_argument_layout" => narrow(
            imp("Move(fFirstPosition, fSecondPosition, fVelocity, fAcceleration);"),
            40,
        ),
        "collection_max_inline" => imp(LONG_CALL),
        "arg_break" => imp("MoveAbsolute(fTargetPosition,\nfMaxVelocity,\nbExecute);"),
        "struct_initializer_max_inline" => {
            decl("VAR\nstPoint : ST_Point := (x := 1, y := 2, z := 3);\nEND_VAR")
        }
        "array_initializer_max_inline" => {
            decl("VAR\naSteps : ARRAY[1..5] OF INT := [10, 20, 30, 40, 50];\nEND_VAR")
        }
        "trailing_comma_collection" => narrow(
            imp("Move(fFirstPosition, fSecondPosition, fVelocity, fAcceleration);"),
            40,
        ),
        "trailing_comma_enum" => decl("TYPE E_State :\n(\nIdle,\nRunning,\nDone\n);\nEND_TYPE"),
        "align_decl_colon" => decl(DECLS),
        "align_at_addresses" => {
            decl("VAR\nbIn AT %I* : BOOL;\nnCounterValue AT %Q* : INT;\nEND_VAR")
        }
        "space_after_line_comment" | "trailing_comment_min_spaces" => {
            imp("nCount := 0; //reset the counter\nbDone := TRUE; //finished")
        }
        "align_trailing_comments" => decl(
            "VAR\nbStart : BOOL; // start request\nnCycleCount : INT := 0; // cycles since reset\nEND_VAR",
        ),
        "blank_lines_max" => imp("nA := 1;\n\n\n\nnB := 2;"),
        "blank_lines_at_block_start" | "blank_lines_at_block_end" => {
            imp("IF bStart THEN\n\n\nnCount := 1;\n\n\nEND_IF")
        }
        "blank_lines_around_block_statement" => {
            imp("nCount := 0;\nIF bStart THEN\nnCount := 1;\nEND_IF\nbDone := TRUE;")
        }
        "blank_lines_after_case" => {
            imp("CASE nState OF\n0: nState := 10;\n10: bDone := TRUE;\nEND_CASE")
        }
        "blank_lines_after_multiline_case" => imp(
            "CASE nState OF\n0:\nnCount := 0;\nnState := 10;\n10:\nbDone := TRUE;\nnState := 0;\nEND_CASE",
        ),
        "blank_lines_before_multiline_comment" => imp(
            "nCount := 0;\n(* Start the motor once\n   the guard is closed. *)\nbStart := bGuardClosed;",
        ),
        "blank_lines_after_decl" => {
            decl("VAR_INPUT\nbEnable : BOOL;\nEND_VAR\nVAR\nnState : INT;\nEND_VAR")
        }
        _ => return None,
    })
}

/// Format the sample of setting `key` with `style` (plus the sample's own
/// width and prerequisites). `None` for an unknown key; `Err` if formatting
/// failed, which the tests rule out for every value a setting accepts.
pub fn render_sample(key: &str, style: &StyleConfig) -> Option<crate::Result<String>> {
    let sample = sample(key)?;
    let mut style = style.clone();
    if let Some(width) = sample.line_width {
        style.line_width = width;
    }
    if let Some(prepare) = sample.prepare {
        prepare(&mut style);
    }
    Some(format_st(
        sample.source,
        sample.section,
        &FormatOptions { style },
    ))
}

/// One setting's sample at its default value (`before`) and at the value it
/// has in the given style (`after`); all other settings as in that style.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub before: String,
    pub after: String,
}

/// The before/after preview of setting `key` for `style`.
pub fn preview(key: &str, style: &StyleConfig) -> Option<crate::Result<Preview>> {
    let option = tctidy_format::options::find_option(key)?;
    let mut baseline = style.clone();
    option
        .set(&mut baseline, &option.default_value())
        .expect("a setting accepts its own default");
    let before = match render_sample(key, &baseline)? {
        Ok(text) => text,
        Err(e) => return Some(Err(e)),
    };
    let after = match render_sample(key, style)? {
        Ok(text) => text,
        Err(e) => return Some(Err(e)),
    };
    Some(Ok(Preview { before, after }))
}

/// A function block's two parts of Structured Text.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Example {
    pub declaration: &'static str,
    pub implementation: &'static str,
}

/// A function block written in the default style, for a settings UI to format
/// with the values chosen: unchanged until one of them differs from its
/// default.
pub const EXAMPLE: Example = Example {
    declaration: r#"FUNCTION_BLOCK FB_Conveyor
VAR_INPUT
    bEnable        : BOOL; // start request from the panel
    bReset         : BOOL; // clears a latched fault
    bEmergencyStop : BOOL;
    fTargetSpeed   : LREAL := 0.5; // m/s
END_VAR
VAR_OUTPUT
    bRunning : BOOL;
    bError   : BOOL;
    nErrorId : UDINT;
END_VAR
VAR
    bGuardClosed   AT %I* : BOOL;
    bAirPressureOk AT %I* : BOOL;
    bMotorOn       AT %Q* : BOOL;
    eState                : E_ConveyorState := E_ConveyorState.Idle;
    fbStartDelay          : TON;
    fbAxis                : MC_MoveVelocity;
    stAxis                : AXIS_REF;
    stLimits              : ST_Limits := (fMin := 0.0, fMax := 2.0);
    aSensors              : ARRAY[1..8] OF BOOL;
    bAnyBlocked           : BOOL;
    fSpeed                : LREAL;
    nCycles               : DINT := 0;
    i                     : INT;
    tStartDelay           : TIME := T#500MS;
END_VAR
"#,
    implementation: r#"IF bReset THEN
    bError := FALSE;
    nErrorId := 0;
    eState := E_ConveyorState.Idle;
END_IF
nCycles := nCycles + 1;

(* The conveyor starts once the guard is closed,
   the air pressure is up and nothing is in fault. *)
CASE eState OF
    E_ConveyorState.Idle:
        bMotorOn := FALSE;
        IF bEnable
           AND bGuardClosed
           AND bAirPressureOk
           AND NOT bEmergencyStop THEN
            eState := E_ConveyorState.Starting;
        END_IF
    E_ConveyorState.Starting:
        fbStartDelay(IN := TRUE,
                     PT := tStartDelay);
        IF fbStartDelay.Q THEN
            eState := E_ConveyorState.Running;
        END_IF
    E_ConveyorState.Running:
        bMotorOn := TRUE;
    E_ConveyorState.Error:
        bMotorOn := FALSE;
END_CASE

fSpeed := LIMIT(stLimits.fMin, fTargetSpeed, stLimits.fMax);
bAnyBlocked := FALSE;
FOR i := 1 TO 8 DO
    bAnyBlocked := bAnyBlocked OR aSensors[i];
END_FOR

fbAxis(Axis     := stAxis,
       Execute  := bMotorOn AND NOT bAnyBlocked,
       Velocity := fSpeed);
bRunning := eState = E_ConveyorState.Running; // drives the output
"#,
};

/// Every setting, grouped as a settings page shows them.
#[derive(Clone, Debug, Serialize)]
pub struct Catalog {
    pub groups: Vec<CatalogGroup>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogGroup {
    pub id: &'static str,
    pub label: &'static str,
    pub settings: Vec<CatalogSetting>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CatalogSetting {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: SettingKind,
    pub default: SettingValue,
    pub value: SettingValue,
}

/// The values a setting accepts, as a UI picks its control.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
    Bool,
    Integer { min: u16, max: u16 },
    Choice { choices: &'static [&'static str] },
}

/// A setting's value as plain JSON (`true`, `4`, `"spaces"`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Integer(i64),
    Choice(String),
}

impl From<OptionValue> for SettingValue {
    fn from(value: OptionValue) -> Self {
        match value {
            OptionValue::Bool(b) => SettingValue::Bool(b),
            OptionValue::Integer(n) => SettingValue::Integer(n),
            OptionValue::Choice(s) => SettingValue::Choice(s),
        }
    }
}

impl From<SettingValue> for OptionValue {
    fn from(value: SettingValue) -> Self {
        match value {
            SettingValue::Bool(b) => OptionValue::Bool(b),
            SettingValue::Integer(n) => OptionValue::Integer(n),
            SettingValue::Choice(s) => OptionValue::Choice(s),
        }
    }
}

/// The catalog of every setting, with its value in `style`.
pub fn catalog(style: &StyleConfig) -> Catalog {
    let groups = OptionGroup::ALL
        .iter()
        .map(|&group| CatalogGroup {
            id: group.id(),
            label: group.label(),
            settings: style_options()
                .iter()
                .filter(|o| o.group == group)
                .map(|o| catalog_setting(o, style))
                .collect(),
        })
        .filter(|g| !g.settings.is_empty())
        .collect();
    Catalog { groups }
}

fn catalog_setting(option: &StyleOption, style: &StyleConfig) -> CatalogSetting {
    CatalogSetting {
        key: option.key,
        label: option.label,
        description: option.description,
        kind: match option.kind {
            OptionKind::Bool => SettingKind::Bool,
            OptionKind::Integer { min, max } => SettingKind::Integer { min, max },
            OptionKind::Choice(choices) => SettingKind::Choice { choices },
        },
        default: option.default_value().into(),
        value: option.get(style).into(),
    }
}

/// A setting a UI sent that could not be applied.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SettingError {
    pub key: String,
    pub message: String,
}

/// Apply setting values keyed by setting key to `style`. Valid entries are
/// applied; every unknown key and invalid value is reported and skipped.
pub fn apply_values<'a>(
    style: &mut StyleConfig,
    values: impl IntoIterator<Item = (&'a str, &'a SettingValue)>,
) -> Vec<SettingError> {
    let mut errors = Vec::new();
    for (key, value) in values {
        let result = match tctidy_format::options::find_option(key) {
            None => Err("unknown setting".to_string()),
            Some(option) => option
                .set(style, &value.clone().into())
                .map_err(|e| e.to_string()),
        };
        if let Err(message) = result {
            errors.push(SettingError {
                key: key.to_string(),
                message,
            });
        }
    }
    errors
}

/// Build a style from setting values, starting from the house style. Fails
/// with every unknown key and invalid value.
pub fn style_from_values<'a>(
    values: impl IntoIterator<Item = (&'a str, &'a SettingValue)>,
) -> Result<StyleConfig, Vec<SettingError>> {
    let mut style = StyleConfig::default();
    let errors = apply_values(&mut style, values);
    if errors.is_empty() {
        Ok(style)
    } else {
        Err(errors)
    }
}

/// The before/after preview of every setting for `style`.
pub fn previews(style: &StyleConfig) -> std::collections::BTreeMap<&'static str, Preview> {
    style_options()
        .iter()
        .filter_map(|o| Some((o.key, preview(o.key, style)?.ok()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value worth trying for a setting: both booleans, the bounds and a
    /// middle integer, every choice.
    fn values(option: &StyleOption) -> Vec<OptionValue> {
        match option.kind {
            OptionKind::Bool => vec![OptionValue::Bool(false), OptionValue::Bool(true)],
            OptionKind::Integer { min, max } => [min, (min + max) / 2, max]
                .into_iter()
                .map(|n| OptionValue::Integer(n.into()))
                .collect(),
            OptionKind::Choice(choices) => choices
                .iter()
                .map(|c| OptionValue::Choice((*c).to_string()))
                .collect(),
        }
    }

    #[test]
    fn every_setting_has_a_sample() {
        for option in style_options() {
            assert!(sample(option.key).is_some(), "no sample for {}", option.key);
        }
    }

    #[test]
    fn every_sample_shows_its_setting() {
        let default = StyleConfig::default();
        for option in style_options() {
            let base = render_sample(option.key, &default).unwrap().unwrap();
            let changes = values(option).iter().any(|value| {
                let mut style = StyleConfig::default();
                option.set(&mut style, value).unwrap();
                render_sample(option.key, &style).unwrap().unwrap() != base
            });
            assert!(
                changes,
                "no value of {} changes its sample:\n{base}",
                option.key
            );
        }
    }

    #[test]
    fn every_value_renders_its_sample_idempotently() {
        for option in style_options() {
            let sample = sample(option.key).unwrap();
            for value in values(option) {
                let mut style = StyleConfig::default();
                option.set(&mut style, &value).unwrap();
                let once = render_sample(option.key, &style)
                    .unwrap()
                    .unwrap_or_else(|e| panic!("{} = {value}: {e}", option.key));
                let mut again = style.clone();
                if let Some(width) = sample.line_width {
                    again.line_width = width;
                }
                if let Some(prepare) = sample.prepare {
                    prepare(&mut again);
                }
                let twice =
                    format_st(&once, sample.section, &FormatOptions { style: again }).unwrap();
                assert_eq!(twice, once, "{} = {value} is not idempotent", option.key);
            }
        }
    }

    #[test]
    fn preview_compares_the_default_with_the_chosen_value() {
        let mut style = StyleConfig::default();
        let option = tctidy_format::options::find_option("keyword_case").unwrap();
        option
            .set(&mut style, &OptionValue::Choice("lower".into()))
            .unwrap();
        let p = preview("keyword_case", &style).unwrap().unwrap();
        assert!(
            p.before.starts_with("IF bStart AND NOT bBusy THEN"),
            "{}",
            p.before
        );
        assert!(
            p.after.starts_with("if bStart and not bBusy then"),
            "{}",
            p.after
        );
        assert!(preview("no_such_setting", &style).is_none());
    }

    #[test]
    fn catalog_lists_every_setting_with_its_value() {
        let style = StyleConfig {
            indent_width: 2,
            ..StyleConfig::default()
        };
        let catalog = catalog(&style);
        let settings: Vec<_> = catalog.groups.iter().flat_map(|g| &g.settings).collect();
        assert_eq!(settings.len(), style_options().len());
        let indent = settings.iter().find(|s| s.key == "indent_width").unwrap();
        assert_eq!(indent.value, SettingValue::Integer(2));
        assert_eq!(indent.default, SettingValue::Integer(4));
    }

    #[test]
    fn the_example_is_in_the_default_style() {
        let options = FormatOptions::default();
        assert_eq!(
            format_st(EXAMPLE.declaration, StSection::Declaration, &options).unwrap(),
            EXAMPLE.declaration
        );
        assert_eq!(
            format_st(EXAMPLE.implementation, StSection::Implementation, &options).unwrap(),
            EXAMPLE.implementation
        );
    }

    #[test]
    fn previews_cover_every_setting() {
        assert_eq!(
            previews(&StyleConfig::default()).len(),
            style_options().len()
        );
    }

    #[test]
    fn style_from_values_reports_every_bad_entry() {
        let good = SettingValue::Integer(2);
        let style = style_from_values([("indent_width", &good)]).unwrap();
        assert_eq!(style.indent_width, 2);

        let bad = SettingValue::Choice("huge".into());
        let errors = style_from_values([("indent_width", &bad), ("nope", &good)]).unwrap_err();
        assert_eq!(
            errors,
            vec![
                SettingError {
                    key: "indent_width".into(),
                    message: "expected an integer".into()
                },
                SettingError {
                    key: "nope".into(),
                    message: "unknown setting".into()
                },
            ]
        );
    }
}

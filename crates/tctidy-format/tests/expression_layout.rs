use tctidy_format::style::{
    CallArgumentLayout, LogicalExpressionLayout, ParenthesizedExpressionLayout,
};
use tctidy_format::{format_implementation, StyleConfig};
use tctidy_pretty::LineEnding;
use tctidy_syntax::{parse_implementation, SyntaxKind, SyntaxNode};

const LOGICAL_REQUEST: &str = "VentOpenRequest := (Climate.TempHigh OR Climate.HumidityHigh) AND NOT RainDetected AND NOT WindAlarm AND ((Co2LevelHigh AND Climate.InAutoMode) OR (Panel.OpenVent AND NOT Panel.CloseVent) OR (ScheduleActive AND (Season.Summer OR Season.Spring) AND NOT Climate.FrostRisk));";

fn fmt(source: &str, style: &StyleConfig) -> String {
    format_implementation(&parse_implementation(source), style, LineEnding::Lf)
        .expect("implementation formats cleanly")
}

fn semantic_tokens(node: &SyntaxNode) -> Vec<(SyntaxKind, String)> {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| {
            let text = if token.kind().is_keyword() {
                token.text().to_ascii_uppercase()
            } else {
                token.text().to_string()
            };
            (token.kind(), text)
        })
        .collect()
}

fn assert_idempotent(source: &str, style: &StyleConfig) -> String {
    let once = fmt(source, style);
    assert_eq!(fmt(&once, style), once);
    once
}

#[test]
fn expanded_default_preserves_semantics_and_is_idempotent() {
    let before = parse_implementation(LOGICAL_REQUEST);
    assert!(before.errors().is_empty(), "{:?}", before.errors());

    let style = StyleConfig::default();
    let once = fmt(LOGICAL_REQUEST, &style);
    let after = parse_implementation(&once);
    assert!(after.errors().is_empty(), "{:?}", after.errors());
    assert_eq!(
        semantic_tokens(&before.syntax_node()),
        semantic_tokens(&after.syntax_node())
    );
    assert_eq!(fmt(&once, &style), once);
}

#[test]
fn compact_layout_keeps_fitting_branches_inline() {
    let style = StyleConfig {
        logical_expression_layout: LogicalExpressionLayout::Compact,
        ..StyleConfig::default()
    };
    let output = fmt(LOGICAL_REQUEST, &style);

    assert!(output.contains("(Co2LevelHigh AND Climate.InAutoMode)"));
    assert!(!output.contains("OR (\n"));
    assert_eq!(fmt(&output, &style), output);
}

#[test]
fn removing_optional_operator_spaces_keeps_keyword_operators_separated() {
    let style = StyleConfig {
        space_around_binary_op: false,
        ..StyleConfig::default()
    };
    assert_eq!(fmt("x := a + b AND c;", &style), "x := a+b AND c;");
}

#[test]
fn long_named_argument_call_is_canonical_in_one_pass() {
    let source = "Mixer(Recipe := ActiveRecipe, Start := BatchRequested AND Lid.Closed AND NOT RecipeLocked, Speed := 1200, Duration := T#45S, Heating := Recipe.NeedsHeat OR Season.Winter, Abort := AbortBatch OR NOT Hall.Safety.CurtainOk);";
    let output = assert_idempotent(source, &StyleConfig::default());

    assert_eq!(
        output,
        concat!(
            "Mixer(Recipe   := ActiveRecipe,\n",
            "      Start    := BatchRequested AND Lid.Closed AND NOT RecipeLocked,\n",
            "      Speed    := 1200,\n",
            "      Duration := T#45S,\n",
            "      Heating  := Recipe.NeedsHeat OR Season.Winter,\n",
            "      Abort    := AbortBatch OR NOT Hall.Safety.CurtainOk);"
        )
    );
    assert!(output.lines().all(|line| line.len() <= 160));
}

#[test]
fn short_call_with_multiple_named_arguments_always_breaks() {
    let source = "Delay(IN := Start, PT := T#200MS, Q => Done);";
    let output = assert_idempotent(source, &StyleConfig::default());

    assert_eq!(
        output,
        concat!(
            "Delay(IN := Start,\n",
            "      PT := T#200MS,\n",
            "      Q  => Done);"
        )
    );
}

#[test]
fn simple_function_calls_stay_inline() {
    let style = StyleConfig::default();

    assert_eq!(fmt("result := MAX(a, b);", &style), "result := MAX(a, b);");
    assert_eq!(
        fmt("Configure(Enable := enabled);", &style),
        "Configure(Enable := enabled);"
    );
}

#[test]
fn vertical_call_argument_layout_remains_available() {
    let source = "Mixer(Recipe := ActiveRecipe, Start := BatchRequested AND Lid.Closed AND NOT RecipeLocked, Abort := AbortBatch OR NOT Hall.Safety.CurtainOk);";
    let style = StyleConfig {
        call_argument_layout: CallArgumentLayout::Vertical,
        ..StyleConfig::default()
    };
    let output = assert_idempotent(source, &style);

    assert_eq!(
        output,
        concat!(
            "Mixer(\n",
            "    Recipe := ActiveRecipe,\n",
            "    Start  := BatchRequested AND Lid.Closed AND NOT RecipeLocked,\n",
            "    Abort  := AbortBatch OR NOT Hall.Safety.CurtainOk\n",
            ");"
        )
    );
}

#[test]
fn long_parenthesized_condition_uses_hanging_layout_by_default() {
    let source = "IF NOT (Doors.North.Closed AND Doors.South.Closed AND Hatch.Roof.Sensor.Closed AND Hatch.Floor.Sensor.Closed AND Gate.Locked) THEN AlarmCount := AlarmCount + 1; END_IF;";
    let output = assert_idempotent(source, &StyleConfig::default());

    assert_eq!(
        output,
        concat!(
            "IF NOT (Doors.North.Closed\n",
            "        AND Doors.South.Closed\n",
            "        AND Hatch.Roof.Sensor.Closed\n",
            "        AND Hatch.Floor.Sensor.Closed\n",
            "        AND Gate.Locked\n",
            "        ) THEN\n",
            "    AlarmCount := AlarmCount + 1;\n",
            "END_IF;"
        )
    );
    assert!(output.lines().all(|line| line.len() <= 160));
}

#[test]
fn vertical_parenthesized_expression_layout_remains_available() {
    let source = "IF NOT (Doors.North.Closed AND Doors.South.Closed AND Hatch.Roof.Sensor.Closed AND Hatch.Floor.Sensor.Closed AND Gate.Locked) THEN AlarmCount := AlarmCount + 1; END_IF;";
    let style = StyleConfig {
        parenthesized_expression_layout: ParenthesizedExpressionLayout::Vertical,
        ..StyleConfig::default()
    };
    let output = assert_idempotent(source, &style);

    assert_eq!(
        output,
        concat!(
            "IF NOT (\n",
            "    Doors.North.Closed\n",
            "    AND Doors.South.Closed\n",
            "    AND Hatch.Roof.Sensor.Closed\n",
            "    AND Hatch.Floor.Sensor.Closed\n",
            "    AND Gate.Locked\n",
            ") THEN\n",
            "    AlarmCount := AlarmCount + 1;\n",
            "END_IF;"
        )
    );
}

#[test]
fn logical_chain_over_the_inline_limit_breaks_one_operand_per_line() {
    // 97 columns, so it fits the default width: the operand count breaks it.
    let source = "IF bLowBattery OR nRetries > nMaxRetries OR eLink <> E_Link.Online OR tSinceLastFrame > T#5S THEN\nx := 1;\nEND_IF";
    assert_eq!(
        assert_idempotent(source, &StyleConfig::default()),
        "IF bLowBattery\n   OR nRetries > nMaxRetries\n   OR eLink <> E_Link.Online\n   OR tSinceLastFrame > T#5S THEN\n    x := 1;\nEND_IF"
    );
}

#[test]
fn logical_chain_within_the_inline_limit_stays_on_one_line() {
    let source = "IF bA OR bB OR bC THEN\nx := 1;\nEND_IF";
    assert_eq!(
        assert_idempotent(source, &StyleConfig::default()),
        "IF bA OR bB OR bC THEN\n    x := 1;\nEND_IF"
    );
}

#[test]
fn logical_chain_limit_counts_each_precedence_level_separately() {
    // `b AND c` is one operand of the OR chain: three operands, inline.
    let mixed = "IF a OR b AND c OR d THEN\nx := 1;\nEND_IF";
    assert_eq!(
        assert_idempotent(mixed, &StyleConfig::default()),
        "IF a OR b AND c OR d THEN\n    x := 1;\nEND_IF"
    );

    // The AND chain breaks; the parenthesized OR chain inside has three operands.
    let nested = "IF bEnable AND NOT bError AND bHomed AND (nMode = 1 OR nMode = 2 OR nMode = 3) THEN\nx := 1;\nEND_IF";
    assert_eq!(
        assert_idempotent(nested, &StyleConfig::default()),
        "IF bEnable\n   AND NOT bError\n   AND bHomed\n   AND (nMode = 1 OR nMode = 2 OR nMode = 3) THEN\n    x := 1;\nEND_IF"
    );
}

#[test]
fn logical_chain_limit_applies_outside_conditions_and_to_every_logical_operator() {
    let style = StyleConfig::default();
    assert_eq!(
        assert_idempotent("bReady := bPowerOk AND bAirOk AND bDoorClosed AND NOT bEStop;", &style),
        "bReady := bPowerOk\n          AND bAirOk\n          AND bDoorClosed\n          AND NOT bEStop;"
    );
    assert_eq!(
        assert_idempotent("bOdd := bA XOR bB XOR bC XOR bD;", &style),
        "bOdd := bA\n        XOR bB\n        XOR bC\n        XOR bD;"
    );
    // Arithmetic chains are left to the line width.
    assert_eq!(
        assert_idempotent("nSum := nA + nB + nC + nD + nE;", &style),
        "nSum := nA + nB + nC + nD + nE;"
    );
}

#[test]
fn logical_chain_limit_zero_leaves_wrapping_to_the_line_width() {
    let style = StyleConfig {
        logical_chain_max_inline: 0,
        ..StyleConfig::default()
    };
    let source = "bReady := bPowerOk AND bAirOk AND bDoorClosed AND NOT bEStop;";
    assert_eq!(assert_idempotent(source, &style), source);
}

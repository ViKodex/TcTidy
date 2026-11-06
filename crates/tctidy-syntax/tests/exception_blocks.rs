use tctidy_syntax::ast::{AstNode, TryStmt};
use tctidy_syntax::{parse_implementation, SyntaxKind};

fn assert_clean(source: &str) -> tctidy_syntax::Parse<tctidy_syntax::ast::ImplPart> {
    let parse = parse_implementation(source);
    assert!(parse.debug_text_matches(source), "tree is not lossless");
    assert!(parse.errors().is_empty(), "{:?}", parse.errors());
    parse
}

#[test]
fn exception_block_has_structured_nodes() {
    let parse = assert_clean(
        "__TRY\n    x := 1;\n__CATCH(exc)\n    y := 2;\n__FINALLY\n    z := 3;\n__ENDTRY",
    );
    let kinds: Vec<_> = parse
        .syntax_node()
        .descendants()
        .map(|node| node.kind())
        .collect();

    assert!(kinds.contains(&SyntaxKind::TRY_STMT));
    assert!(kinds.contains(&SyntaxKind::CATCH_CLAUSE));
    assert!(kinds.contains(&SyntaxKind::FINALLY_CLAUSE));

    let try_stmt = parse
        .syntax_node()
        .descendants()
        .find_map(TryStmt::cast)
        .expect("typed exception node");
    assert_eq!(try_stmt.try_token().unwrap().text(), "__TRY");
    assert!(try_stmt.body().is_some());
    assert_eq!(
        try_stmt
            .catch_clause()
            .unwrap()
            .exception_variable()
            .unwrap()
            .text(),
        "exc"
    );
    assert!(try_stmt.finally_clause().unwrap().body().is_some());
    assert_eq!(try_stmt.endtry_token().unwrap().text(), "__ENDTRY");
}

#[test]
fn catch_and_finally_are_independently_optional() {
    for source in [
        "__TRY\n    x := 1;\n__CATCH(exc)\n    y := 2;\n__ENDTRY",
        "__TRY\n    x := 1;\n__FINALLY\n    y := 2;\n__ENDTRY",
    ] {
        assert_clean(source);
    }
}

#[test]
fn exception_blocks_nest() {
    let parse = assert_clean(
        "__TRY\n    __TRY\n        x := 1;\n    __CATCH(inner)\n        y := 2;\n    __ENDTRY\n__CATCH(outer)\n    z := 3;\n__ENDTRY",
    );
    assert_eq!(
        parse
            .syntax_node()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::TRY_STMT)
            .count(),
        2
    );
}

#[test]
fn malformed_exception_blocks_report_errors() {
    for source in [
        "__TRY\n    x := 1;\n__CATCH(exc)",
        "__TRY\n    x := 1;\n__CATCH exc\n__ENDTRY",
        "__TRY\n    x := 1;\n__ENDTRY",
    ] {
        assert!(
            !parse_implementation(source).errors().is_empty(),
            "malformed block accepted:\n{source}"
        );
    }
}

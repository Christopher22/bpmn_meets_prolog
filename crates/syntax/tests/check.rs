//! Behavioural tests for `bpmn_syntax::check`.

use bpmn_syntax::{Clause, ErrorKind, Head, Position, check};

const PIZZA: &str = include_str!("../../../prolog/pizza.pl");
const BPMN: &str = include_str!("../../../prolog/bpmn.pl");

fn error(source: &str) -> (u32, u32, ErrorKind) {
    let error = check(source).expect_err("source should be rejected");
    (error.position.line, error.position.column, error.kind)
}

fn heads(source: &str) -> Vec<Option<String>> {
    check(source)
        .expect("source should be accepted")
        .clauses()
        .iter()
        .map(|clause| clause.head().map(|head| head.to_string()))
        .collect()
}

#[test]
fn accepts_the_bundled_programs() {
    let pizza = check(PIZZA).unwrap();
    assert!(pizza.defines("pizza", 1));
    assert!(pizza.defines("main", 0));
    assert!(!pizza.defines("model", 1));
    assert_eq!(pizza.clauses().len(), 7);

    let library = check(BPMN).unwrap();
    assert!(library.defines("bpmn_db", 2));
    assert!(library.defines("cat", 2));
}

#[test]
fn splits_clauses_and_records_positions() {
    let program = check("a.\n  b :- c.  % trailing\n/* c. */ d(\"x.\").").unwrap();
    let texts: Vec<_> = program.clauses().iter().map(Clause::text).collect();
    assert_eq!(texts, ["a.", "b :- c.", "d(\"x.\")."]);
    let positions: Vec<_> = program.clauses().iter().map(Clause::position).collect();
    assert_eq!(
        positions,
        [
            Position { line: 1, column: 1 },
            Position { line: 2, column: 3 },
            Position {
                line: 3,
                column: 10
            },
        ]
    );
}

#[test]
fn recognises_heads() {
    assert_eq!(
        heads(
            ":- use_module(bpmn).\nmodel(M) :- f(M, [a, b], g(c, d)).\nmain.\n'quoted'(x, y).\nf (x).\nVar.\n"
        ),
        [
            None,
            Some("model/1".to_owned()),
            Some("main/0".to_owned()),
            Some("quoted/2".to_owned()),
            Some("f/0".to_owned()),
            None,
        ]
    );
    let program = check("model(bpmn(x, [])).").unwrap();
    assert_eq!(
        program.clauses()[0].head(),
        Some(Head {
            name: "model",
            arity: 1
        })
    );
}

#[test]
fn end_token_needs_layout_after_the_dot() {
    assert_eq!(
        heads("X =.. Y.\nf(1.5e-3, 0.5).\ng :- X = 'a.b'.%c"),
        [None, Some("f/2".to_owned()), Some("g/0".to_owned()),]
    );
    assert_eq!(error("a.b"), (1, 4, ErrorKind::MissingEnd));
}

#[test]
fn handles_character_codes_and_escapes() {
    let program = "a(0'a, 0''', 0'\\n, 0' , 0'ä).\nb(\"say \\\"hi\\\"\", 'it''s', \"\\x41\\\").\nc(\"line \\\n continued\").";
    assert_eq!(check(program).unwrap().clauses().len(), 3);
}

#[test]
fn reports_unterminated_items_at_their_start() {
    assert_eq!(
        error("a.\nb(\"open)."),
        (2, 3, ErrorKind::UnterminatedQuoted { quote: '"' })
    );
    assert_eq!(
        error("a('x)."),
        (1, 3, ErrorKind::UnterminatedQuoted { quote: '\'' })
    );
    assert_eq!(
        error("a. /* x"),
        (1, 4, ErrorKind::UnterminatedBlockComment)
    );
    assert_eq!(error("a(0'"), (1, 3, ErrorKind::IncompleteCharacterCode));
}

#[test]
fn rejects_raw_newlines_in_quoted_items() {
    assert_eq!(
        error("a(\"x\ny\")."),
        (1, 5, ErrorKind::NewlineInQuoted { quote: '"' })
    );
}

#[test]
fn reports_bracket_errors() {
    assert_eq!(
        error("a([b)."),
        (
            1,
            5,
            ErrorKind::MismatchedCloser {
                expected: ']',
                found: ')'
            }
        )
    );
    assert_eq!(
        error("a)."),
        (1, 2, ErrorKind::UnexpectedCloser { found: ')' })
    );
    assert_eq!(
        error("a(b, c(d.\ne."),
        (1, 7, ErrorKind::UnclosedBracket { open: '(' })
    );
    assert_eq!(
        error("a(b"),
        (1, 2, ErrorKind::UnclosedBracket { open: '(' })
    );
}

#[test]
fn reports_missing_and_empty_clauses() {
    assert_eq!(
        error("a.\nb :- c   % no end\n"),
        (2, 7, ErrorKind::MissingEnd)
    );
    assert_eq!(error("a. ."), (1, 4, ErrorKind::EmptyClause));
}

#[test]
fn counts_columns_in_characters() {
    assert_eq!(
        error("ä(ö, ü]."),
        (
            1,
            7,
            ErrorKind::MismatchedCloser {
                expected: ')',
                found: ']'
            }
        )
    );
}

#[test]
fn rejects_control_characters() {
    assert_eq!(
        error("a\u{7}."),
        (1, 2, ErrorKind::UnexpectedCharacter { found: '\u{7}' })
    );
}

#[test]
fn accepts_empty_and_comment_only_input() {
    assert_eq!(check("").unwrap().clauses(), []);
    assert_eq!(check("% nothing\n/* here */\n").unwrap().clauses(), []);
}

#[test]
fn error_messages_are_readable() {
    let error = check("a([b).").unwrap_err();
    assert_eq!(error.to_string(), "1:5: expected ']' but found ')'");
}

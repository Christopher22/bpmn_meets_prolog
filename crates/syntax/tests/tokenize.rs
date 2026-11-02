//! Behavioural tests for `bpmn_syntax::tokenize`.

use bpmn_syntax::{ErrorKind, TokenKind, check, tokenize};

const PIZZA: &str = include_str!("../../../prolog/pizza.pl");
const BPMN: &str = include_str!("../../../prolog/bpmn.pl");

fn kinds(source: &str) -> Vec<(TokenKind, &str)> {
    tokenize(source)
        .map(|token| (token.kind, token.text))
        .collect()
}

/// Tokens plus the layout between them reproduce the source exactly.
fn assert_lossless(source: &str) {
    let mut rebuilt = String::new();
    let mut end = 0;
    for token in tokenize(source) {
        let layout = &source[end..token.span.start];
        assert!(
            layout.chars().all(char::is_whitespace),
            "non-layout gap {layout:?}"
        );
        rebuilt.push_str(layout);
        rebuilt.push_str(token.text);
        end = token.span.end;
    }
    assert!(source[end..].chars().all(char::is_whitespace));
    rebuilt.push_str(&source[end..]);
    assert_eq!(rebuilt, source);
}

#[test]
fn classifies_tokens() {
    use TokenKind::*;
    assert_eq!(
        kinds("model(M) :- task(t, \"T\", 'q'), X = [1.5|_], f (y), 0'a, `b`. /* c */"),
        [
            (Functor, "model"),
            (Punctuation, "("),
            (Variable, "M"),
            (Punctuation, ")"),
            (Symbol, ":-"),
            (Functor, "task"),
            (Punctuation, "("),
            (Atom, "t"),
            (Punctuation, ","),
            (String, "\"T\""),
            (Punctuation, ","),
            (QuotedAtom, "'q'"),
            (Punctuation, ")"),
            (Punctuation, ","),
            (Variable, "X"),
            (Symbol, "="),
            (Punctuation, "["),
            (Number, "1.5"),
            (Punctuation, "|"),
            (Variable, "_"),
            (Punctuation, "]"),
            (Punctuation, ","),
            (Atom, "f"),
            (Punctuation, "("),
            (Atom, "y"),
            (Punctuation, ")"),
            (Punctuation, ","),
            (CharCode, "0'a"),
            (Punctuation, ","),
            (BackQuoted, "`b`"),
            (End, "."),
            (Comment, "/* c */"),
        ]
    );
}

#[test]
fn distinguishes_the_end_token_from_symbols() {
    use TokenKind::*;
    assert_eq!(
        kinds("a =.. b.%x\nc."),
        [
            (Atom, "a"),
            (Symbol, "=.."),
            (Atom, "b"),
            (End, "."),
            (Comment, "%x"),
            (Atom, "c"),
            (End, "."),
        ]
    );
}

#[test]
fn recovers_after_lexical_errors() {
    use TokenKind::*;
    let tokens = kinds("a(\"open\nb(\u{7}).\n/* tail");
    assert_eq!(
        tokens,
        [
            (Functor, "a"),
            (Punctuation, "("),
            (Invalid(ErrorKind::NewlineInQuoted { quote: '"' }), "\"open"),
            (Functor, "b"),
            (Punctuation, "("),
            (
                Invalid(ErrorKind::UnexpectedCharacter { found: '\u{7}' }),
                "\u{7}"
            ),
            (Punctuation, ")"),
            (End, "."),
            (Invalid(ErrorKind::UnterminatedBlockComment), "/* tail"),
        ]
    );
}

#[test]
fn is_lossless() {
    assert_lossless(PIZZA);
    assert_lossless(BPMN);
    assert_lossless("ä(ö, \"ü\\\n x\", 'it''s'). % ünïcode\n\t\n");
    assert_lossless("broken(\"x\n  more 'stuff");
}

#[test]
fn agrees_with_check_on_errors() {
    let source = "a.\nb(\"x\ny\").";
    let error = check(source).unwrap_err();
    let invalid = tokenize(source)
        .find(|token| matches!(token.kind, TokenKind::Invalid(_)))
        .unwrap();
    assert_eq!(TokenKind::Invalid(error.kind), invalid.kind);
    // The raw newline is reported where it is.
    assert_eq!(error.offset, invalid.span.end);
    assert_eq!(&source[error.offset..=error.offset], "\n");
}

//! Renaming ids, atoms, predicates and variables.
//!
//! Everything works on tokens, so strings and comments never match. Variables
//! are renamed within their clause, names within the whole program. A name
//! covers its atoms, simple quoted atoms and functors alike, because
//! `maplist(helper, L)` refers to `helper(X)`.
//!
//! All functions work on byte offsets and return an [`Edit`] for the editor to
//! apply, so they are plain, testable string logic.

use std::fmt;
use std::ops::Range;

use bpmn_syntax::{Token, TokenKind, tokenize};

use crate::indent::Edit;
use crate::vocabulary;

/// Names that rendering depends on: `model/1` and `use_module(bpmn)`.
const RESERVED: [&str; 2] = ["model", "bpmn"];

/// The class of identifier being renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Name,
    Variable,
}

/// What a rename at the cursor would change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) name: String,
    /// Every occurrence in source order.
    pub(crate) occurrences: Vec<Range<usize>>,
    kind: Kind,
    /// The bytes in which the name has to be unique.
    scope: Range<usize>,
    /// The index of the occurrence at the cursor.
    at: usize,
}

/// Why there is nothing to rename at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Refusal {
    NotAName,
    Anonymous,
    Dsl(String),
    Reserved(String),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAName => write!(f, "Put the caret on an id, atom or variable to rename it."),
            Self::Anonymous => write!(f, "The anonymous variable _ cannot be renamed."),
            Self::Dsl(name) => write!(f, "{name} is part of the BPMN DSL and cannot be renamed."),
            Self::Reserved(name) => {
                write!(f, "{name} is needed for rendering and cannot be renamed.")
            }
        }
    }
}

/// Why a new name cannot be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Invalid {
    NotAnAtom,
    NotAVariable,
    Reserved,
    Taken,
}

impl fmt::Display for Invalid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAnAtom => {
                "Must start with a lowercase letter and contain only letters, digits and _."
            }
            Self::NotAVariable => {
                "Must start with an uppercase letter or _ and contain only letters, digits and _."
            }
            Self::Reserved => "This name is reserved by the BPMN DSL.",
            Self::Taken => "This name is already in use.",
        })
    }
}

/// The identifier at byte offset `cursor` of `source` and its occurrences.
pub(crate) fn target(source: &str, cursor: usize) -> Result<Target, Refusal> {
    let tokens: Vec<Token<'_>> = tokenize(source).collect();
    let token = tokens
        .iter()
        .find(|token| {
            token.span.start <= cursor && cursor <= token.span.end && is_identifier(token.kind)
        })
        .ok_or(Refusal::NotAName)?;

    let (name, kind) = match token.kind {
        TokenKind::Variable if token.text == "_" => return Err(Refusal::Anonymous),
        TokenKind::Variable => (token.text, Kind::Variable),
        TokenKind::Functor if vocabulary::lookup(token.text).next().is_some() => {
            return Err(Refusal::Dsl(token.text.to_owned()));
        }
        TokenKind::QuotedAtom => (unquoted(token.text).ok_or(Refusal::NotAName)?, Kind::Name),
        _ => (token.text, Kind::Name),
    };
    if kind == Kind::Name {
        if vocabulary::is_value(name) {
            return Err(Refusal::Dsl(name.to_owned()));
        }
        if RESERVED.contains(&name) {
            return Err(Refusal::Reserved(name.to_owned()));
        }
    }

    let scope = match kind {
        Kind::Name => 0..source.len(),
        Kind::Variable => clause_around(&tokens, token, source.len()),
    };
    let occurrences = occurrences(&tokens, name, kind, &scope);
    let at = occurrences
        .iter()
        .position(|span| *span == token.span)
        .unwrap_or(0);
    Ok(Target {
        name: name.to_owned(),
        occurrences,
        kind,
        scope,
        at,
    })
}

/// Checks that `target` can be renamed to `new_name`. Keeping the name is fine.
pub(crate) fn validate(source: &str, target: &Target, new_name: &str) -> Result<(), Invalid> {
    if new_name == target.name {
        return Ok(());
    }
    let tokens: Vec<Token<'_>> = tokenize(source).collect();
    let mut lexed = tokenize(new_name);
    let single = lexed.next().filter(|token| token.text == new_name);
    let kind = single.map(|token| token.kind);
    match target.kind {
        Kind::Name if kind != Some(TokenKind::Atom) => return Err(Invalid::NotAnAtom),
        Kind::Name if vocabulary::is_value(new_name) || RESERVED.contains(&new_name) => {
            return Err(Invalid::Reserved);
        }
        Kind::Variable if kind != Some(TokenKind::Variable) || new_name == "_" => {
            return Err(Invalid::NotAVariable);
        }
        _ => {}
    }
    if occurrences(&tokens, new_name, target.kind, &target.scope).is_empty() {
        Ok(())
    } else {
        Err(Invalid::Taken)
    }
}

/// The edit renaming `target` to `new_name`, as one replacement from the first
/// to the last occurrence so that it is a single undo step. It selects the
/// renamed occurrence that was at the cursor.
pub(crate) fn edit(source: &str, target: &Target, new_name: &str) -> Edit {
    let start = target.occurrences.first().map_or(0, |span| span.start);
    let mut insert = String::new();
    let mut select = start..start;
    let mut end = start;
    for (index, span) in target.occurrences.iter().enumerate() {
        insert.push_str(&source[end..span.start]);
        if index == target.at {
            let word = start + insert.len();
            select = word..word + new_name.len();
        }
        insert.push_str(new_name);
        end = span.end;
    }
    Edit {
        replace: start..end,
        insert,
        select,
    }
}

/// The byte offset of the first atom called `id`.
pub(crate) fn first_occurrence(source: &str, id: &str) -> Option<usize> {
    tokenize(source)
        .find(|token| is_name(token, id) && token.kind != TokenKind::Functor)
        .map(|token| token.span.start)
}

fn is_identifier(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Atom | TokenKind::Functor | TokenKind::QuotedAtom | TokenKind::Variable
    )
}

/// The text of a quoted atom, unless it has escapes that would need decoding.
fn unquoted(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
    (!inner.is_empty() && !inner.contains(['\\', '\''])).then_some(inner)
}

/// Whether `token` spells the name `name`. DSL functors never do: in
/// `start(start)`, only the argument is an id.
fn is_name(token: &Token<'_>, name: &str) -> bool {
    match token.kind {
        TokenKind::Atom => token.text == name,
        TokenKind::Functor => token.text == name && vocabulary::lookup(name).next().is_none(),
        TokenKind::QuotedAtom => unquoted(token.text) == Some(name),
        _ => false,
    }
}

fn occurrences(
    tokens: &[Token<'_>],
    name: &str,
    kind: Kind,
    scope: &Range<usize>,
) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter(|token| scope.contains(&token.span.start))
        .filter(|token| match kind {
            Kind::Name => is_name(token, name),
            Kind::Variable => token.kind == TokenKind::Variable && token.text == name,
        })
        .map(|token| token.span.clone())
        .collect()
}

/// The bytes of the clause containing `token`.
fn clause_around(tokens: &[Token<'_>], token: &Token<'_>, len: usize) -> Range<usize> {
    let ends = || tokens.iter().filter(|t| t.kind == TokenKind::End);
    let start = ends()
        .take_while(|end| end.span.end <= token.span.start)
        .last()
        .map_or(0, |end| end.span.end);
    let end = ends()
        .find(|end| end.span.start >= token.span.end)
        .map_or(len, |end| end.span.end);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renames what is at `|` in `marked` to `new_name`.
    fn rename(marked: &str, new_name: &str) -> String {
        let cursor = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let target = target(&source, cursor).unwrap();
        validate(&source, &target, new_name).unwrap();
        let edit = edit(&source, &target, new_name);
        let mut result = source;
        result.replace_range(edit.replace.clone(), &edit.insert);
        assert_eq!(&result[edit.select], new_name);
        result
    }

    fn refusal(marked: &str) -> Refusal {
        let cursor = marked.find('|').unwrap();
        target(&marked.replacen('|', "", 1), cursor).unwrap_err()
    }

    fn invalid(marked: &str, new_name: &str) -> Invalid {
        let cursor = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let target = target(&source, cursor).unwrap();
        validate(&source, &target, new_name).unwrap_err()
    }

    #[test]
    fn renames_every_reference_but_not_text() {
        assert_eq!(
            rename(
                "task(or|der, \"order\"), % order\nlane(l, \"L\", [order]), a => order, order ~> b.",
                "buy"
            ),
            "task(buy, \"order\"), % order\nlane(l, \"L\", [buy]), a => buy, buy ~> b."
        );
    }

    #[test]
    fn caret_may_touch_either_end() {
        assert_eq!(rename("a(|b, b).", "c"), "a(c, c).");
        assert_eq!(rename("a(b|, b).", "c"), "a(c, c).");
    }

    #[test]
    fn matches_simple_quoted_atoms() {
        assert_eq!(rename("a(|b, 'b', 'b c').", "c"), "a(c, c, 'b c').");
        assert_eq!(rename("a('|b c', 'b c', b).", "d"), "a(d, d, b).");
        assert_eq!(refusal("a('|it''s')."), Refusal::NotAName);
    }

    #[test]
    fn name_covers_predicates_and_atoms() {
        assert_eq!(
            rename("hel|per(1).\nq :- maplist(helper, L), helper(2).", "aid"),
            "aid(1).\nq :- maplist(aid, L), aid(2)."
        );
    }

    #[test]
    fn variables_stay_in_their_clause() {
        assert_eq!(
            rename("p(X) :- q(X).\nr(|X) :- s(X), t(x).\nu(X).", "Y"),
            "p(X) :- q(X).\nr(Y) :- s(Y), t(x).\nu(X)."
        );
        // The last clause may lack its end while typing.
        assert_eq!(rename("p(X).\nq(|X, X)", "Y"), "p(X).\nq(Y, Y)");
    }

    #[test]
    fn dsl_stays_intact() {
        assert_eq!(
            rename("start(|start), a => start.", "go"),
            "start(go), a => go."
        );
        assert_eq!(refusal("ta|sk(a, \"A\")."), Refusal::Dsl("task".to_owned()));
        assert_eq!(
            refusal("ti|mer(\"PT1H\")."),
            Refusal::Dsl("timer".to_owned())
        );
        assert_eq!(
            refusal("task(a, \"A\", se|nd)."),
            Refusal::Dsl("send".to_owned())
        );
        assert_eq!(refusal("mo|del(a)."), Refusal::Reserved("model".to_owned()));
        assert_eq!(
            refusal(":- use_module(bp|mn)."),
            Refusal::Reserved("bpmn".to_owned())
        );
    }

    #[test]
    fn refuses_what_is_no_identifier() {
        assert_eq!(refusal("a(\"te|xt\")."), Refusal::NotAName);
        assert_eq!(refusal("% com|ment"), Refusal::NotAName);
        assert_eq!(refusal("a(1|2)."), Refusal::NotAName);
        assert_eq!(refusal("a =|> b."), Refusal::NotAName);
        assert_eq!(refusal("a(|_)."), Refusal::Anonymous);
    }

    #[test]
    fn validates_new_names() {
        for bad in [
            "",
            "Order",
            "two words",
            " padded",
            "a(",
            "1a",
            "'quoted'",
            "_x",
        ] {
            assert_eq!(invalid("a(|b).", bad), Invalid::NotAnAtom, "{bad:?}");
        }
        assert_eq!(invalid("a(|b).", "send"), Invalid::Reserved);
        assert_eq!(invalid("a(|b).", "model"), Invalid::Reserved);
        assert_eq!(invalid("a(|b, c).", "c"), Invalid::Taken);
        assert_eq!(invalid("q(|b), c(1).", "c"), Invalid::Taken);

        for bad in ["", "x", "_", "A b", "1"] {
            assert_eq!(invalid("a(|X).", bad), Invalid::NotAVariable, "{bad:?}");
        }
        assert_eq!(invalid("a(|X, Y).", "Y"), Invalid::Taken);
        // The same variable name in another clause is a different variable.
        assert_eq!(rename("a(|X).\nb(Y).", "Y"), "a(Y).\nb(Y).");
        assert_eq!(rename("a(|X, b).", "_Y"), "a(_Y, b).");
    }

    #[test]
    fn dsl_element_names_are_free_as_ids() {
        // Only the functor exists, so the id `start` does not collide with it.
        assert_eq!(rename("start(a), |b.", "start"), "start(a), start.");
        assert_eq!(rename("a(|b).", "b"), "a(b).");
    }

    #[test]
    fn offsets_are_bytes() {
        assert_eq!(
            rename("n(\"Benötigt\", ä|b), m(\"𝄞\", äb).", "ö"),
            "n(\"Benötigt\", ö), m(\"𝄞\", ö)."
        );
    }

    #[test]
    fn finds_the_first_atom() {
        let source = "a(1). \"b\". % b\nb(2). lane([b]). x(b).";
        assert_eq!(
            first_occurrence(source, "b"),
            source.find("[b").map(|i| i + 1)
        );
        assert_eq!(first_occurrence(source, "zzz"), None);
    }
}

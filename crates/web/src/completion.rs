//! Completion of DSL elements and declared element ids at the cursor.

use std::collections::BTreeMap;
use std::ops::Range;

use bpmn_syntax::{Token, TokenKind, tokenize};

use crate::vocabulary::{self, Category, ENTRIES};

/// At most this many suggestions are offered.
const LIMIT: usize = 50;

/// Suggestions for the word at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Completion {
    /// The byte range an accepted item replaces (the whole word).
    pub(crate) replace: Range<usize>,
    pub(crate) items: Vec<Item>,
}

/// One suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) label: String,
    pub(crate) detail: &'static str,
    pub(crate) insert: String,
    /// The byte range within `insert` to select after inserting it.
    pub(crate) select: Option<Range<usize>>,
}

/// Rank of a suggestion's source: declared ids before DSL elements before
/// DSL values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Id,
    Element,
    Value,
}

/// Suggestions for the cursor at byte offset `cursor` of `source`.
///
/// Without `explicit` (that is, while typing) suggestions need a word prefix;
/// with it (Ctrl+Space) an empty prefix offers everything. Nothing is offered
/// inside comments, strings, quoted atoms and character codes.
pub(crate) fn complete(source: &str, cursor: usize, explicit: bool) -> Option<Completion> {
    let tokens: Vec<Token<'_>> = tokenize(source).collect();
    let at_cursor = tokens
        .iter()
        .find(|token| token.span.start < cursor && cursor <= token.span.end);

    let replace = match at_cursor {
        Some(token) if inside_text(token, cursor) => return None,
        Some(token) if matches!(token.kind, TokenKind::Atom | TokenKind::Functor) => {
            token.span.clone()
        }
        Some(token) if token.kind == TokenKind::Variable => return None,
        _ => cursor..cursor,
    };
    let prefix = &source[replace.start..cursor];
    if prefix.is_empty() && !explicit {
        return None;
    }
    // `ta|sk(` keeps its argument list: insert the bare name.
    let bare = source[replace.end..].starts_with('(');

    let mut candidates: Vec<(Rank, Item)> = declared_ids(&tokens, &replace)
        .into_iter()
        .map(|(id, detail)| {
            let item = Item {
                label: id.to_owned(),
                detail,
                insert: id.to_owned(),
                select: None,
            };
            (Rank::Id, item)
        })
        .collect();
    candidates.extend(ENTRIES.iter().map(|entry| {
        let rank = match entry.category {
            Category::Element => Rank::Element,
            Category::Value => Rank::Value,
        };
        let item = if bare {
            Item {
                label: entry.name.to_owned(),
                detail: entry.detail,
                insert: entry.name.to_owned(),
                select: None,
            }
        } else {
            Item {
                label: entry.template.to_owned(),
                detail: entry.detail,
                insert: entry.template.to_owned(),
                select: entry.first_argument(),
            }
        };
        (rank, item)
    }));

    candidates.retain(|(_, item)| item.insert.starts_with(prefix) && item.insert != prefix);
    candidates.sort_by(|(a, x), (b, y)| (a, &x.label).cmp(&(b, &y.label)));
    candidates.dedup_by(|(_, x), (_, y)| x.insert == y.insert);
    candidates.truncate(LIMIT);

    let items: Vec<Item> = candidates.into_iter().map(|(_, item)| item).collect();
    (!items.is_empty()).then_some(Completion { replace, items })
}

/// Whether `cursor` is within the text of a comment, quoted item or
/// character code (where words are not code).
fn inside_text(token: &Token<'_>, cursor: usize) -> bool {
    match token.kind {
        // A line comment runs to the end of its line, cursor included.
        TokenKind::Comment => token.text.starts_with('%') || cursor < token.span.end,
        TokenKind::String | TokenKind::QuotedAtom | TokenKind::BackQuoted => {
            cursor < token.span.end
        }
        TokenKind::CharCode | TokenKind::Invalid(_) => true,
        _ => false,
    }
}

/// Ids declared as the first argument of DSL elements, e.g. `choose` in
/// `task(choose, "Select a pizza", user)`, with the element's description.
/// The word being completed (`editing`) does not count as a declaration.
fn declared_ids<'a>(tokens: &[Token<'a>], editing: &Range<usize>) -> Vec<(&'a str, &'static str)> {
    let mut ids = BTreeMap::new();
    for window in tokens.windows(3) {
        let [functor, open, id] = window else {
            continue;
        };
        if functor.kind != TokenKind::Functor
            || open.text != "("
            || id.kind != TokenKind::Atom
            || id.span == *editing
        {
            continue;
        }
        if let Some(entry) = vocabulary::lookup(functor.text).find(|entry| entry.declares_id) {
            // Keep the first declaration of each id.
            ids.entry(id.text).or_insert(entry.detail);
        }
    }
    ids.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "model(bpmn(m, [pool(p, \"P\", [\n    task(choose, \"Choose\", user),\n    start(go),\n    lane(l, \"L\", [go]),\n    go => cho\n])])).\n% ta in a comment\nx(\"ta\").";

    fn labels(completion: &Completion) -> Vec<&str> {
        completion
            .items
            .iter()
            .map(|item| item.label.as_str())
            .collect()
    }

    fn at(needle: &str) -> usize {
        MODEL.find(needle).unwrap() + needle.len()
    }

    #[test]
    fn offers_declared_ids_first() {
        let completion = complete(MODEL, at("go => cho"), false).unwrap();
        assert_eq!(labels(&completion), ["choose"]);
        assert_eq!(completion.items[0].detail, "task");
        assert_eq!(&MODEL[completion.replace.clone()], "cho");

        // Ctrl+Space before a word: every declared id, but lanes declare none.
        let completion = complete(MODEL, at("go => "), true).unwrap();
        assert_eq!(labels(&completion)[..3], ["choose", "go", "p"]);
        assert!(!labels(&completion).contains(&"l"));
    }

    #[test]
    fn offers_dsl_templates_with_the_first_argument_selected() {
        let source = "x([ta";
        let completion = complete(source, source.len(), false).unwrap();
        assert_eq!(
            labels(&completion),
            ["task(Id, \"Name\")", "task(Id, \"Name\", user)"]
        );
        let item = &completion.items[0];
        assert_eq!(&item.insert[item.select.clone().unwrap()], "Id");
        assert_eq!(completion.replace, 3..5);

        let source = "x(st";
        assert_eq!(
            labels(&complete(source, source.len(), false).unwrap()),
            ["start(Id, \"Name\")", "start(Id, \"Name\", message)",]
        );
    }

    #[test]
    fn inserts_bare_names_before_an_argument_list() {
        let source = "x(ta(a, \"A\"))";
        let completion = complete(source, 4, false).unwrap();
        assert_eq!(labels(&completion), ["task"]);
        assert_eq!(completion.items[0].select, None);
        assert_eq!(completion.replace, 2..4);
    }

    #[test]
    fn stays_quiet_where_words_are_not_code() {
        assert_eq!(complete(MODEL, at("% ta"), false), None);
        assert_eq!(complete(MODEL, at("x(\"ta"), false), None);
        assert_eq!(complete("x(Ta", 4, false), None);
        // Nothing to complete, or nothing to add.
        assert_eq!(complete("x(", 2, false), None);
        assert_eq!(complete("x(qqq", 5, false), None);
        assert_eq!(complete("x(user", 6, false), None);
    }

    #[test]
    fn explicit_requests_work_without_a_prefix() {
        let completion = complete("x(", 2, true).unwrap();
        assert_eq!(completion.replace, 2..2);
        assert_eq!(completion.items.len(), LIMIT.min(ENTRIES.len()));
        assert_eq!(completion.items[0].label, "and(Id)");
    }
}

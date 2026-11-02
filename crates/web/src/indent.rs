//! Whitespace-aware editing: Enter keeps the indentation (one level deeper
//! after an opening bracket), Tab and Shift+Tab indent and outdent, and
//! Backspace removes indentation one level at a time.
//!
//! All functions work on byte offsets and return an [`Edit`] for the editor
//! to apply, so they are plain, testable string logic.

use std::ops::Range;

use bpmn_syntax::{TokenKind, tokenize};

/// Columns per indentation level, as in the example program and `tab-size`.
pub(crate) const UNIT: usize = 4;

const INDENT: &str = "    ";

/// Replace `replace` with `insert`, then select `select` (byte offsets into
/// the text after the edit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) replace: Range<usize>,
    pub(crate) insert: String,
    pub(crate) select: Range<usize>,
}

/// A line break at `selection` that keeps the current indentation.
///
/// After an opening bracket the new line is indented one level deeper, and a
/// matching closer right after the caret moves to its own line at the
/// original level.
pub(crate) fn newline(text: &str, selection: Range<usize>) -> Edit {
    let before = &text[line_start(text, selection.start)..selection.start];
    let indent = leading(before);
    let mut insert = format!("\n{indent}");

    let opener = tokenize(before)
        .last()
        .filter(|token| token.kind == TokenKind::Punctuation)
        .and_then(|token| closer(token.text));
    if opener.is_some() {
        insert.push_str(INDENT);
    }
    let caret = selection.start + insert.len();
    if let Some(closer) = opener
        && text[selection.end..].starts_with(closer)
    {
        insert.push('\n');
        insert.push_str(indent);
    }
    Edit {
        replace: selection,
        insert,
        select: caret..caret,
    }
}

/// Tab: spaces to the next indentation stop, or one more level for every
/// line of a multi-line selection.
pub(crate) fn indent(text: &str, selection: Range<usize>) -> Edit {
    if !text[selection.clone()].contains('\n') {
        let column = columns(&text[line_start(text, selection.start)..selection.start]);
        let insert = " ".repeat(UNIT - column % UNIT);
        let caret = selection.start + insert.len();
        return Edit {
            replace: selection,
            insert,
            select: caret..caret,
        };
    }
    let block = lines(text, &selection);
    let insert = text[block.clone()]
        .split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                line.to_owned()
            } else {
                format!("{INDENT}{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Edit {
        select: block.start..block.start + insert.len(),
        replace: block,
        insert,
    }
}

/// Shift+Tab: one level less for every line touched by `selection`.
/// `None` if there is nothing to remove.
pub(crate) fn outdent(text: &str, selection: Range<usize>) -> Option<Edit> {
    let block = lines(text, &selection);
    let mut first_removed = None;
    let insert = text[block.clone()]
        .split('\n')
        .map(|line| {
            let width = if line.starts_with('\t') {
                1
            } else {
                line.bytes().take(UNIT).take_while(|&b| b == b' ').count()
            };
            first_removed.get_or_insert(width);
            &line[width..]
        })
        .collect::<Vec<_>>()
        .join("\n");
    if insert.len() == block.len() {
        return None;
    }
    let select = if selection.is_empty() {
        let caret = selection
            .start
            .saturating_sub(first_removed.unwrap_or(0))
            .max(block.start);
        caret..caret
    } else {
        block.start..block.start + insert.len()
    };
    Some(Edit {
        replace: block,
        insert,
        select,
    })
}

/// Backspace in leading spaces: back to the previous indentation stop.
/// `None` where a plain backspace does the right thing.
pub(crate) fn backspace(text: &str, selection: Range<usize>) -> Option<Edit> {
    if !selection.is_empty() {
        return None;
    }
    let before = &text[line_start(text, selection.start)..selection.start];
    if before.is_empty() || !before.bytes().all(|b| b == b' ') {
        return None;
    }
    let width = (before.len() - 1) % UNIT + 1;
    let from = selection.start - width;
    (width > 1).then(|| Edit {
        replace: from..selection.start,
        insert: String::new(),
        select: from..from,
    })
}

/// Columns taken by `line` in the editor (tabs stop every [`UNIT`] columns).
pub(crate) fn columns(line: &str) -> usize {
    line.chars().fold(0, |column, c| match c {
        '\t' => (column / UNIT + 1) * UNIT,
        _ => column + 1,
    })
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |newline| newline + 1)
}

fn line_end(text: &str, at: usize) -> usize {
    text[at..]
        .find('\n')
        .map_or(text.len(), |newline| at + newline)
}

/// The whole lines touched by `selection`. A selection ending at the start
/// of a line does not include that line.
fn lines(text: &str, selection: &Range<usize>) -> Range<usize> {
    let last = if selection.end > selection.start && text[..selection.end].ends_with('\n') {
        selection.end - 1
    } else {
        selection.end
    };
    line_start(text, selection.start)..line_end(text, last)
}

fn leading(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn closer(open: &str) -> Option<&'static str> {
    match open {
        "(" => Some(")"),
        "[" => Some("]"),
        "{" => Some("}"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `edit` to `text`, marking the resulting selection with `|`
    /// (caret) or `[` … `]` (range).
    fn apply(text: &str, edit: &Edit) -> String {
        let mut result = text.to_owned();
        result.replace_range(edit.replace.clone(), &edit.insert);
        let Range { start, end } = edit.select;
        if start == end {
            result.insert(start, '|');
        } else {
            result.insert(end, ']');
            result.insert(start, '[');
        }
        result
    }

    /// Splits `marked` at `|` (caret) or `[` … `]` into text and selection.
    fn parse(marked: &str) -> (String, Range<usize>) {
        if let Some(caret) = marked.find('|') {
            return (marked.replacen('|', "", 1), caret..caret);
        }
        let start = marked.find('[').unwrap();
        let end = marked.find(']').unwrap() - 1;
        (marked.replacen('[', "", 1).replacen(']', "", 1), start..end)
    }

    fn check(f: impl Fn(&str, Range<usize>) -> Option<Edit>, before: &str, after: Option<&str>) {
        let (text, selection) = parse(before);
        let result = f(&text, selection).map(|edit| apply(&text, &edit));
        assert_eq!(result.as_deref(), after, "for {before:?}");
    }

    #[test]
    fn enter_keeps_the_indentation() {
        let enter = |t: &str, s| Some(newline(t, s));
        check(enter, "    task(a),|", Some("    task(a),\n    |"));
        check(enter, "\ttask(a),|", Some("\ttask(a),\n\t|"));
        check(enter, "x|", Some("x\n|"));
        // The caret inside the indentation keeps only what is before it.
        check(enter, "  |  x", Some("  \n  |  x"));
        // A selection is replaced.
        check(enter, "    a[bc]d", Some("    a\n    |d"));
    }

    #[test]
    fn enter_after_an_opener_goes_one_level_deeper() {
        let enter = |t: &str, s| Some(newline(t, s));
        check(
            enter,
            "    pool(c, \"C\", [|",
            Some("    pool(c, \"C\", [\n        |"),
        );
        check(enter, "  f(|)", Some("  f(\n      |\n  )"));
        check(enter, "a(\"[\"|", Some("a(\"[\"\n|"));
        check(enter, "x % [|", Some("x % [\n|"));
    }

    #[test]
    fn tab_inserts_spaces_to_the_next_stop() {
        let tab = |t: &str, s| Some(indent(t, s));
        check(tab, "|x", Some("    |x"));
        check(tab, "ab|", Some("ab  |"));
        check(tab, "    |", Some("        |"));
        check(tab, "a[bc]d", Some("a   |d"));
    }

    #[test]
    fn tab_indents_selected_lines() {
        let tab = |t: &str, s| Some(indent(t, s));
        check(tab, "a\nb[c\n\nd]e\nf", Some("a\n[    bc\n\n    de]\nf"));
        // The line after a trailing newline is not touched.
        check(tab, "[a\n]b", Some("[    a]\nb"));
    }

    #[test]
    fn shift_tab_outdents() {
        check(outdent, "    a|b", Some("a|b"));
        check(outdent, "      a|", Some("  a|"));
        check(outdent, "\ta|", Some("a|"));
        check(outdent, "  |  a", Some("|a"));
        check(outdent, "a|", None);
        check(outdent, "[    a\n  b\nc]", Some("[a\nb\nc]"));
    }

    #[test]
    fn backspace_removes_a_level() {
        check(backspace, "        |x", Some("    |x"));
        check(backspace, "      |", Some("    |"));
        check(backspace, " |", None);
        check(backspace, "    a|", None);
        check(backspace, "[    ]", None);
    }

    #[test]
    fn counts_columns() {
        assert_eq!(columns("ab"), 2);
        assert_eq!(columns("\tx"), 5);
        assert_eq!(columns("ab\tx"), 5);
    }
}

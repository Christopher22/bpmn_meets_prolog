//! Syntax highlighting: the editor text as HTML for the overlay behind the
//! text area.

use std::ops::Range;

use bpmn_syntax::{SyntaxError, Token, TokenKind, tokenize};

use crate::vocabulary::{self, Category};

/// Renders `source` as HTML with one `<span class="t-…">` per token.
///
/// The text itself is unchanged (only escaped), so the overlay lines up
/// with the text area character for character. The token at `error` is
/// underlined; an error between tokens (such as a missing `.`) gets a
/// zero-width marker. Tokens at the spans in `marked` are highlighted.
pub(crate) fn to_html(
    source: &str,
    error: Option<&SyntaxError>,
    marked: &[Range<usize>],
) -> String {
    let mut html = String::with_capacity(source.len() * 3 / 2);
    let mut pending_error = error.map(|error| error.offset);
    let mut end = 0;

    for token in tokenize(source) {
        layout(&mut html, source, end..token.span.start, &mut pending_error);
        let flagged = pending_error
            .take_if(|at| token.span.contains(at))
            .is_some();
        html.push_str("<span class=\"");
        html.push_str(class(&token));
        if flagged {
            html.push_str(" t-error");
        }
        if marked.contains(&token.span) {
            html.push_str(" t-rename");
        }
        html.push_str("\">");
        escape(&mut html, token.text);
        html.push_str("</span>");
        end = token.span.end;
    }
    layout(&mut html, source, end..source.len(), &mut pending_error);
    if pending_error.is_some() {
        html.push_str(MARKER);
    }
    // A text area shows a final empty line; a <pre> needs a newline for it.
    html.push('\n');
    html
}

const MARKER: &str = "<span class=\"t-mark\"></span>";

/// Writes the layout in `range`, with the error marker if it falls inside.
fn layout(html: &mut String, source: &str, range: Range<usize>, error: &mut Option<usize>) {
    match error.take_if(|at| range.contains(at)) {
        Some(at) => {
            escape(html, &source[range.start..at]);
            html.push_str(MARKER);
            escape(html, &source[at..range.end]);
        }
        None => escape(html, &source[range]),
    }
}

fn class(token: &Token<'_>) -> &'static str {
    let in_vocabulary = |category| vocabulary::lookup(token.text).any(|e| e.category == category);
    match token.kind {
        TokenKind::Comment => "t-comment",
        TokenKind::Functor if in_vocabulary(Category::Element) => "t-dsl",
        TokenKind::Functor | TokenKind::Atom if in_vocabulary(Category::Value) => "t-value",
        TokenKind::Functor => "t-functor",
        TokenKind::Atom => "t-atom",
        TokenKind::Variable => "t-var",
        TokenKind::Number | TokenKind::CharCode => "t-num",
        TokenKind::String | TokenKind::BackQuoted => "t-str",
        TokenKind::QuotedAtom => "t-quoted",
        TokenKind::Punctuation => "t-punct",
        TokenKind::Symbol | TokenKind::End => "t-sym",
        _ => "t-invalid",
    }
}

fn escape(html: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => html.push_str("&amp;"),
            '<' => html.push_str("&lt;"),
            '>' => html.push_str("&gt;"),
            c => html.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlay's text content must equal the source.
    fn text_of(html: &str) -> String {
        let mut text = String::new();
        let mut in_tag = false;
        for c in html.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => text.push(c),
                _ => {}
            }
        }
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    #[test]
    fn classifies_and_escapes() {
        let html = to_html("model(M) :- task(t, \"a<b\", user), M @< 1.", None, &[]);
        assert!(html.starts_with("<span class=\"t-functor\">model</span>"));
        assert!(html.contains("<span class=\"t-dsl\">task</span>"));
        assert!(html.contains("<span class=\"t-value\">user</span>"));
        assert!(html.contains("<span class=\"t-str\">\"a&lt;b\"</span>"));
        assert!(html.contains("<span class=\"t-sym\">@&lt;</span>"));
        assert!(html.contains("<span class=\"t-var\">M</span>"));
    }

    #[test]
    fn preserves_the_text() {
        for source in [
            include_str!("example.pl"),
            "a(\"open\nb & c. % <x>\n",
            "  \n\tx.",
        ] {
            assert_eq!(text_of(&to_html(source, None, &[])), format!("{source}\n"));
        }
    }

    #[test]
    fn flags_errors() {
        let source = "a([b).";
        let error = bpmn_syntax::check(source).unwrap_err();
        assert!(
            to_html(source, Some(&error), &[]).contains("<span class=\"t-punct t-error\">)</span>")
        );

        let source = "a :- b   ";
        let error = bpmn_syntax::check(source).unwrap_err();
        let html = to_html(source, Some(&error), &[]);
        assert!(
            html.contains(&format!("<span class=\"t-atom\">b</span>{MARKER}   ")),
            "{html}"
        );
    }
}

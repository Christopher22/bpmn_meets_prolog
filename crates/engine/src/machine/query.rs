//! Builds the Prolog query that hands a program to the driver.

use std::fmt::Write as _;

use bpmn_syntax::Program;

/// `catch(once(driver:render([c(Line, "clause"), ...], R)), _, R = crashed).`
///
/// The query always succeeds exactly once: Scryer's library API cannot
/// safely drop a query that ended in an exception.
pub(crate) fn render(program: &Program<'_>) -> String {
    let clauses = program.clauses();
    let mut query = String::with_capacity(program.source().len() + 16 * clauses.len() + 64);
    query.push_str("catch(once(driver:render([");
    for (index, clause) in clauses.iter().enumerate() {
        if index > 0 {
            query.push(',');
        }
        let _ = write!(query, "c({},", clause.position().line);
        push_string(&mut query, clause.text());
        query.push(')');
    }
    query.push_str("],R)),_,R=crashed).");
    query
}

/// Appends `text` as a double-quoted Prolog string (a list of chars).
fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\x{:x}\\", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_clause_text() {
        let program = bpmn_syntax::check("a(\"q\\\"\", 'x').\n\tb(\"\u{1}\").").unwrap();
        assert_eq!(
            render(&program),
            r#"catch(once(driver:render([c(1,"a(\"q\\\"\", 'x')."),c(2,"b(\"\x1\\").")],R)),_,R=crashed)."#
        );
    }

    #[test]
    fn handles_empty_programs() {
        let program = bpmn_syntax::check("% empty").unwrap();
        assert_eq!(
            render(&program),
            "catch(once(driver:render([],R)),_,R=crashed)."
        );
    }
}

//! Turns the driver's answer term into Rust values.
//!
//! The driver answers with atoms rather than char lists for all text.

use scryer_prolog::Term;

use crate::{Diagnostic, EngineError, Outcome, Problem, Rendering};

type Result<T> = std::result::Result<T, EngineError>;

pub(crate) fn rendering(term: &Term) -> Result<Rendering> {
    match term {
        Term::Compound(name, args) if name == "result" => match args.as_slice() {
            [outcome_term, diagnostics] => Ok(Rendering {
                outcome: outcome(outcome_term)?,
                diagnostics: list(diagnostics, diagnostic)?,
                output: String::new(),
            }),
            _ => Err(unexpected(term)),
        },
        Term::Atom(name) if name == "crashed" => Err(EngineError::Crashed),
        _ => Err(unexpected(term)),
    }
}

fn outcome(term: &Term) -> Result<Outcome> {
    match term {
        Term::Atom(name) if name == "no_model" => Ok(Outcome::NoModel),
        Term::Compound(name, args) => match (name.as_str(), args.as_slice()) {
            // The XML is written to a stream by a separate query.
            ("rendered", [problems]) => Ok(Outcome::Rendered {
                xml: String::new(),
                problems: list(problems, problem)?,
            }),
            ("failed", [message]) => Ok(Outcome::Failed {
                message: text(message)?,
            }),
            _ => Err(unexpected(term)),
        },
        _ => Err(unexpected(term)),
    }
}

fn problem(term: &Term) -> Result<Problem> {
    match term {
        Term::Compound(name, args) if name == "problem" => match args.as_slice() {
            [Term::Atom(code), subjects] => Ok(Problem {
                code: code.clone(),
                subjects: list(subjects, text)?,
            }),
            _ => Err(unexpected(term)),
        },
        _ => Err(unexpected(term)),
    }
}

fn diagnostic(term: &Term) -> Result<Diagnostic> {
    match term {
        Term::Compound(name, args) if name == "diagnostic" => match args.as_slice() {
            [Term::Integer(line), message] => Ok(Diagnostic {
                // `Integer` is arbitrary precision; line numbers fit a u32.
                line: line.to_string().parse().map_err(|_| unexpected(term))?,
                message: text(message)?,
            }),
            _ => Err(unexpected(term)),
        },
        _ => Err(unexpected(term)),
    }
}

fn text(term: &Term) -> Result<String> {
    match term {
        Term::Atom(text) => Ok(text.clone()),
        _ => Err(unexpected(term)),
    }
}

fn list<T>(term: &Term, item: impl Fn(&Term) -> Result<T>) -> Result<Vec<T>> {
    match term {
        Term::List(items) => items.iter().map(item).collect(),
        // Scryer answers any list of one-char atoms, such as `[e]`, as a string.
        Term::String(chars) => chars
            .chars()
            .map(|c| item(&Term::Atom(c.to_string())))
            .collect(),
        Term::Atom(name) if name == "[]" => Ok(Vec::new()),
        _ => Err(unexpected(term)),
    }
}

fn unexpected(term: &Term) -> EngineError {
    EngineError::UnexpectedAnswer(format!("{term:?}"))
}

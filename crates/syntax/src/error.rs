use std::error::Error;
use std::fmt;

use crate::Position;

/// The first lexical error found in a program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    /// Where the error was detected.
    pub position: Position,
    /// The same place as a byte offset into the source.
    pub offset: usize,
    /// What went wrong.
    pub kind: ErrorKind,
}

/// The kinds of errors detected by [`check`](crate::check).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A quoted item is not closed before the end of the input.
    UnterminatedQuoted {
        /// The opening quote character.
        quote: char,
    },
    /// A quoted item contains a raw newline (use `\n` or a `\` continuation).
    NewlineInQuoted {
        /// The opening quote character.
        quote: char,
    },
    /// A `0'` character code is not followed by a character.
    IncompleteCharacterCode,
    /// A `/*` comment is not closed.
    UnterminatedBlockComment,
    /// A closing bracket without a matching opening bracket.
    UnexpectedCloser {
        /// The closing bracket.
        found: char,
    },
    /// A closing bracket that does not match the innermost open bracket.
    MismatchedCloser {
        /// The closer the innermost open bracket needs.
        expected: char,
        /// The closer that was found.
        found: char,
    },
    /// An opening bracket that is still open at the end of its clause.
    UnclosedBracket {
        /// The opening bracket.
        open: char,
    },
    /// The input ends inside a clause that has no end token.
    MissingEnd,
    /// An end token without any preceding term.
    EmptyClause,
    /// A character that cannot start any Prolog token.
    UnexpectedCharacter {
        /// The offending character.
        found: char,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedQuoted { quote } => write!(f, "unterminated {quote}…{quote}"),
            Self::NewlineInQuoted { quote } => write!(
                f,
                "newline inside {quote}…{quote}; use \\n or end the line with \\"
            ),
            Self::IncompleteCharacterCode => f.write_str("incomplete character code after 0'"),
            Self::UnterminatedBlockComment => f.write_str("unterminated /* comment"),
            Self::UnexpectedCloser { found } => write!(f, "unexpected '{found}'"),
            Self::MismatchedCloser { expected, found } => {
                write!(f, "expected '{expected}' but found '{found}'")
            }
            Self::UnclosedBracket { open } => {
                write!(f, "'{open}' is not closed before the end of the clause")
            }
            Self::MissingEnd => f.write_str("clause is not terminated by '.'"),
            Self::EmptyClause => f.write_str("'.' without a preceding term"),
            Self::UnexpectedCharacter { found } => write!(f, "unexpected character {found:?}"),
        }
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.position, self.kind)
    }
}

impl Error for SyntaxError {}

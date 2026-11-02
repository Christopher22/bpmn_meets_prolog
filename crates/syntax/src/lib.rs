//! A fast lexical pre-check for Prolog source text.
//!
//! [`check`] scans a program once, byte by byte, and verifies everything that
//! can be decided without an operator table:
//!
//! * quoted atoms, strings, back-quoted strings and `0'c` character codes are
//!   terminated (and do not contain raw newlines),
//! * block comments are closed,
//! * `()`, `[]` and `{}` are balanced and correctly nested,
//! * every clause is terminated by an end token (`.` followed by layout,
//!   `%` or the end of input).
//!
//! It is deliberately *not* a parser: operator priorities and term structure
//! are left to Prolog. In exchange it runs in linear time, allocates only the
//! clause list and the bracket stack, and is cheap enough to run on every
//! keystroke.
//!
//! On success it returns a [`Program`] that splits the source into
//! [`Clause`]s and records each clause's syntactic [`Head`].
//!
//! The same lexer is available on its own through [`tokenize`], which never
//! stops at errors and so suits syntax highlighting while the user types.
//!
//! ```
//! let program = bpmn_syntax::check("model(bpmn(m, [])).\n% done\n").unwrap();
//! assert!(program.defines("model", 1));
//!
//! let error = bpmn_syntax::check("model(bpmn(m, [)).").unwrap_err();
//! assert_eq!(error.to_string(), "1:16: expected ']' but found ')'");
//! ```

mod error;
mod lexer;
mod scanner;

pub use error::{ErrorKind, SyntaxError};

use std::fmt;
use std::ops::Range;

/// A 1-based line/column position. Columns count Unicode scalar values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// Line number, starting at 1.
    pub line: u32,
    /// Column number, starting at 1.
    pub column: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// The syntactic head of a clause: its leading atom and argument count.
///
/// For `model(M) :- build(M).` this is `model/1`; for `main :- run.` it is
/// `main/0`. Directives have no head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Head<'a> {
    /// Name of the predicate.
    pub name: &'a str,
    /// Number of arguments.
    pub arity: usize,
}

impl fmt::Display for Head<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.name, self.arity)
    }
}

/// One clause or directive, including its terminating `.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause<'a> {
    text: &'a str,
    position: Position,
    head: Option<Head<'a>>,
}

impl<'a> Clause<'a> {
    /// Source text from the first token up to and including the end token.
    #[must_use]
    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Position of the clause's first token.
    #[must_use]
    pub fn position(&self) -> Position {
        self.position
    }

    /// The clause's syntactic head, if it starts with an atom.
    #[must_use]
    pub fn head(&self) -> Option<Head<'a>> {
        self.head
    }
}

/// A lexically valid program, split into clauses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program<'a> {
    source: &'a str,
    clauses: Vec<Clause<'a>>,
}

impl<'a> Program<'a> {
    /// The complete source text.
    #[must_use]
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// The clauses in source order.
    #[must_use]
    pub fn clauses(&self) -> &[Clause<'a>] {
        &self.clauses
    }

    /// Whether any clause has the head `name/arity`.
    #[must_use]
    pub fn defines(&self, name: &str, arity: usize) -> bool {
        self.clauses
            .iter()
            .filter_map(Clause::head)
            .any(|head| head.name == name && head.arity == arity)
    }
}

/// The lexical class of a [`Token`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// `% …` or `/* … */`.
    Comment,
    /// A name atom such as `task`.
    Atom,
    /// A name atom directly followed by `(`, such as `task` in `task(t, "T")`.
    Functor,
    /// A variable such as `Model` or `_`.
    Variable,
    /// An integer or float.
    Number,
    /// `"…"`.
    String,
    /// `'…'`.
    QuotedAtom,
    /// `` `…` ``.
    BackQuoted,
    /// `0'c`.
    CharCode,
    /// One of `( ) [ ] { } , |`.
    Punctuation,
    /// A symbol atom or solo character such as `:-`, `=>`, `!` or `;`.
    Symbol,
    /// The `.` that ends a clause.
    End,
    /// Text that cannot be lexed; lexing resumes after it.
    Invalid(ErrorKind),
}

/// A token of Prolog source text. Layout between tokens is not a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'a> {
    /// The token's class.
    pub kind: TokenKind,
    /// The token's text.
    pub text: &'a str,
    /// The token's byte range in the source.
    pub span: Range<usize>,
}

/// Splits `source` into tokens, including comments, in a single pass.
///
/// Unlike [`check`], this never stops: lexical errors become
/// [`TokenKind::Invalid`] tokens, and an unterminated quoted item ends at the
/// end of its line.
///
/// ```
/// use bpmn_syntax::{TokenKind, tokenize};
///
/// let kinds: Vec<_> = tokenize("a => task(B). % c").map(|t| t.kind).collect();
/// assert_eq!(kinds, [
///     TokenKind::Atom, TokenKind::Symbol, TokenKind::Functor, TokenKind::Punctuation,
///     TokenKind::Variable, TokenKind::Punctuation, TokenKind::End, TokenKind::Comment,
/// ]);
/// ```
pub fn tokenize(source: &str) -> impl Iterator<Item = Token<'_>> {
    lexer::Lexer::new(source).map(|lexeme| {
        let span = lexeme.start.offset..lexeme.end.offset;
        Token {
            kind: lexeme.kind,
            text: &source[span.clone()],
            span,
        }
    })
}

/// Checks the lexical structure of `source` and splits it into clauses.
///
/// # Errors
///
/// Returns the first [`SyntaxError`] found, with its position.
pub fn check(source: &str) -> Result<Program<'_>, SyntaxError> {
    scanner::Scanner::new(source).run()
}

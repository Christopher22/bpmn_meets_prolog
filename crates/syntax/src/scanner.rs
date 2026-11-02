//! The structural checks behind [`check`](crate::check): bracket nesting,
//! clause ends and clause heads, on top of the [lexer](crate::lexer).

use std::mem;

use crate::lexer::{Lexeme, Lexer, Mark};
use crate::{Clause, ErrorKind, Head, Program, SyntaxError, TokenKind};

/// What the head tracker needs to know about a token.
#[derive(Debug, Clone, Copy)]
enum Token<'a> {
    /// An atom that may name a predicate: a lowercase name or a quoted atom
    /// without escapes.
    Atom(&'a str),
    Open,
    Close,
    Comma,
    Other,
}

/// Recognises `name` and `name(Arg, ...)` at the start of a clause.
#[derive(Debug, Clone, Copy)]
enum HeadState<'a> {
    /// No token seen yet in the current clause.
    Expecting,
    /// The clause starts with an atom ending at byte `end`.
    Name {
        name: &'a str,
        end: usize,
    },
    /// Inside the head's argument list, which opened at bracket `depth`.
    Args {
        name: &'a str,
        arity: usize,
        depth: usize,
    },
    Done(Option<Head<'a>>),
}

pub(crate) struct Scanner<'a> {
    src: &'a str,
    brackets: Vec<(u8, Mark)>,
    clauses: Vec<Clause<'a>>,
    clause_start: Option<Mark>,
    last_token_end: Mark,
    head: HeadState<'a>,
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self {
            src,
            brackets: Vec::new(),
            clauses: Vec::new(),
            clause_start: None,
            last_token_end: Mark::ORIGIN,
            head: HeadState::Expecting,
        }
    }

    pub(crate) fn run(mut self) -> Result<Program<'a>, SyntaxError> {
        for lexeme in Lexer::new(self.src) {
            let text = &self.src[lexeme.start.offset..lexeme.end.offset];
            let token = match lexeme.kind {
                TokenKind::Comment => continue,
                TokenKind::Invalid(kind) => {
                    // A raw newline is reported where it is, not at the quote.
                    let at = match kind {
                        ErrorKind::NewlineInQuoted { .. } => lexeme.end,
                        _ => lexeme.start,
                    };
                    return Err(self.error(at, kind));
                }
                TokenKind::End => {
                    self.end(lexeme)?;
                    continue;
                }
                TokenKind::Punctuation => match text.as_bytes()[0] {
                    open @ (b'(' | b'[' | b'{') => {
                        self.brackets.push((open, lexeme.start));
                        Token::Open
                    }
                    close @ (b')' | b']' | b'}') => {
                        self.close(close, lexeme.start)?;
                        Token::Close
                    }
                    b',' => Token::Comma,
                    _ => Token::Other,
                },
                TokenKind::Atom | TokenKind::Functor => Token::Atom(text),
                TokenKind::QuotedAtom => match &text[1..text.len() - 1] {
                    inner if !inner.contains(['\\', '\'']) => Token::Atom(inner),
                    _ => Token::Other,
                },
                _ => Token::Other,
            };
            self.on_token(token, lexeme);
        }

        if let Some(&(open, mark)) = self.brackets.last() {
            return Err(self.error(
                mark,
                ErrorKind::UnclosedBracket {
                    open: char::from(open),
                },
            ));
        }
        if self.clause_start.is_some() {
            return Err(self.error(self.last_token_end, ErrorKind::MissingEnd));
        }
        Ok(Program {
            source: self.src,
            clauses: self.clauses,
        })
    }

    fn on_token(&mut self, token: Token<'a>, lexeme: Lexeme) {
        self.clause_start.get_or_insert(lexeme.start);
        self.last_token_end = lexeme.end;
        self.head = match self.head {
            HeadState::Expecting => match token {
                Token::Atom(name) => HeadState::Name {
                    name,
                    end: lexeme.end.offset,
                },
                _ => HeadState::Done(None),
            },
            HeadState::Name { name, end } => match token {
                // `name(` only opens an argument list without layout in between.
                Token::Open if lexeme.start.offset == end => HeadState::Args {
                    name,
                    arity: 1,
                    depth: self.brackets.len(),
                },
                _ => HeadState::Done(Some(Head { name, arity: 0 })),
            },
            HeadState::Args { name, arity, depth } => match token {
                Token::Comma if self.brackets.len() == depth => HeadState::Args {
                    name,
                    arity: arity + 1,
                    depth,
                },
                Token::Close if self.brackets.len() < depth => {
                    HeadState::Done(Some(Head { name, arity }))
                }
                _ => self.head,
            },
            HeadState::Done(_) => self.head,
        };
    }

    fn end(&mut self, lexeme: Lexeme) -> Result<(), SyntaxError> {
        if let Some(&(open, mark)) = self.brackets.last() {
            return Err(self.error(
                mark,
                ErrorKind::UnclosedBracket {
                    open: char::from(open),
                },
            ));
        }
        let Some(start) = self.clause_start.take() else {
            return Err(self.error(lexeme.start, ErrorKind::EmptyClause));
        };
        let head = match mem::replace(&mut self.head, HeadState::Expecting) {
            HeadState::Name { name, .. } => Some(Head { name, arity: 0 }),
            HeadState::Done(head) => head,
            // `Args` cannot survive an empty bracket stack.
            HeadState::Expecting | HeadState::Args { .. } => None,
        };
        self.clauses.push(Clause {
            text: &self.src[start.offset..lexeme.end.offset],
            position: start.position(self.src),
            head,
        });
        Ok(())
    }

    fn close(&mut self, byte: u8, at: Mark) -> Result<(), SyntaxError> {
        let found = char::from(byte);
        match self.brackets.pop() {
            None => Err(self.error(at, ErrorKind::UnexpectedCloser { found })),
            Some((open, _)) if closer(open) == byte => Ok(()),
            Some((open, _)) => Err(self.error(
                at,
                ErrorKind::MismatchedCloser {
                    expected: char::from(closer(open)),
                    found,
                },
            )),
        }
    }

    fn error(&self, mark: Mark, kind: ErrorKind) -> SyntaxError {
        SyntaxError {
            position: mark.position(self.src),
            offset: mark.offset,
            kind,
        }
    }
}

fn closer(open: u8) -> u8 {
    match open {
        b'(' => b')',
        b'[' => b']',
        _ => b'}',
    }
}

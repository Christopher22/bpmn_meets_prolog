//! The lenient, single-pass lexer behind [`check`](crate::check) and
//! [`tokenize`](crate::tokenize).
//!
//! The lexer works on bytes: every character with a lexical meaning in Prolog
//! is ASCII, and all non-ASCII bytes are treated as alphanumeric, so token
//! boundaries always fall on UTF-8 character boundaries.
//!
//! It never stops: a lexical error becomes a [`TokenKind::Invalid`] token and
//! lexing continues after it. An unterminated quoted item ends at the end of
//! its line, so one missing quote does not swallow the rest of the program.

use crate::{ErrorKind, Position, TokenKind};

/// A byte offset plus the line bookkeeping needed to compute its
/// [`Position`] lazily (columns are only counted when actually needed).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark {
    pub(crate) offset: usize,
    pub(crate) line: u32,
    pub(crate) line_start: usize,
}

impl Mark {
    pub(crate) const ORIGIN: Self = Self {
        offset: 0,
        line: 1,
        line_start: 0,
    };

    /// The 1-based position of this mark in `source`.
    pub(crate) fn position(self, source: &str) -> Position {
        let column = source.as_bytes()[self.line_start..self.offset]
            .iter()
            .filter(|&&b| !is_utf8_continuation(b))
            .count()
            + 1;
        Position {
            line: self.line,
            column: u32::try_from(column).unwrap_or(u32::MAX),
        }
    }
}

/// One token with the marks around it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Lexeme {
    pub(crate) kind: TokenKind,
    pub(crate) start: Mark,
    pub(crate) end: Mark,
}

pub(crate) struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    line_start: usize,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    fn token(&mut self, byte: u8) -> TokenKind {
        match byte {
            b'%' => {
                // A line comment never contains a newline, so the line
                // bookkeeping is unaffected by skipping it in one step.
                let rest = &self.bytes[self.pos..];
                self.pos += rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
                TokenKind::Comment
            }
            b'/' if self.peek_at(1) == Some(b'*') => self.block_comment(),
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b'|' => {
                self.bump();
                TokenKind::Punctuation
            }
            b'!' | b';' => {
                self.bump();
                TokenKind::Symbol
            }
            b'\'' => self.quoted(byte, TokenKind::QuotedAtom),
            b'"' => self.quoted(byte, TokenKind::String),
            b'`' => self.quoted(byte, TokenKind::BackQuoted),
            b'0' if self.peek_at(1) == Some(b'\'') => self.char_code(),
            b'0'..=b'9' => {
                self.number();
                TokenKind::Number
            }
            _ if is_alphanumeric(byte) => {
                let start = self.pos;
                self.eat_while(is_alphanumeric);
                let first = self.src[start..].chars().next();
                if byte == b'_' || first.is_some_and(char::is_uppercase) {
                    TokenKind::Variable
                } else if self.peek() == Some(b'(') {
                    TokenKind::Functor
                } else {
                    TokenKind::Atom
                }
            }
            _ if is_symbol(byte) => {
                if self.symbol() == "." && self.at_end_boundary() {
                    TokenKind::End
                } else {
                    TokenKind::Symbol
                }
            }
            _ => {
                self.bump();
                TokenKind::Invalid(ErrorKind::UnexpectedCharacter {
                    found: char::from(byte),
                })
            }
        }
    }

    fn block_comment(&mut self) -> TokenKind {
        self.bump();
        self.bump();
        loop {
            match self.peek() {
                None => return TokenKind::Invalid(ErrorKind::UnterminatedBlockComment),
                Some(b'*') if self.peek_at(1) == Some(b'/') => {
                    self.bump();
                    self.bump();
                    return TokenKind::Comment;
                }
                Some(_) => self.bump(),
            }
        }
    }

    /// Scans a quoted item; a raw newline or the end of input ends it early.
    fn quoted(&mut self, quote: u8, kind: TokenKind) -> TokenKind {
        let quote_char = char::from(quote);
        self.bump();
        loop {
            match self.peek() {
                None => {
                    return TokenKind::Invalid(ErrorKind::UnterminatedQuoted { quote: quote_char });
                }
                Some(b'\n') => {
                    return TokenKind::Invalid(ErrorKind::NewlineInQuoted { quote: quote_char });
                }
                Some(b'\\') => {
                    self.bump();
                    if !self.escape() {
                        return TokenKind::Invalid(ErrorKind::UnterminatedQuoted {
                            quote: quote_char,
                        });
                    }
                }
                Some(byte) if byte == quote => {
                    self.bump();
                    // A doubled quote stands for the quote itself.
                    if self.peek() == Some(quote) {
                        self.bump();
                    } else {
                        return kind;
                    }
                }
                Some(_) => self.bump(),
            }
        }
    }

    /// Scans the escape sequence after a backslash. `false` at end of input.
    fn escape(&mut self) -> bool {
        match self.peek() {
            None => false,
            Some(b'x') => {
                self.bump();
                self.eat_while(|b| b.is_ascii_hexdigit());
                self.eat(b'\\');
                true
            }
            Some(b'0'..=b'7') => {
                self.eat_while(|b| matches!(b, b'0'..=b'7'));
                self.eat(b'\\');
                true
            }
            // Includes `\` + newline, the line continuation.
            Some(_) => {
                self.bump_char();
                true
            }
        }
    }

    fn char_code(&mut self) -> TokenKind {
        self.bump();
        self.bump();
        match self.peek() {
            None | Some(b'\n') => TokenKind::Invalid(ErrorKind::IncompleteCharacterCode),
            Some(b'\\') => {
                self.bump();
                if self.escape() {
                    TokenKind::CharCode
                } else {
                    TokenKind::Invalid(ErrorKind::UnterminatedQuoted { quote: '\'' })
                }
            }
            Some(b'\'') => {
                // ISO writes the quote as 0''' ; many systems also accept 0''.
                self.bump();
                self.eat(b'\'');
                TokenKind::CharCode
            }
            Some(_) => {
                self.bump_char();
                TokenKind::CharCode
            }
        }
    }

    fn number(&mut self) {
        // Covers integers, `0x..`/`0o..`/`0b..` and digit groups like `1_000`.
        self.eat_while(is_alphanumeric);
        if self.peek() == Some(b'.') && self.peek_at(1).is_some_and(|b| b.is_ascii_digit()) {
            self.bump();
            self.eat_while(is_alphanumeric);
            if matches!(self.bytes[self.pos - 1], b'e' | b'E')
                && matches!(self.peek(), Some(b'+' | b'-'))
            {
                self.bump();
                self.eat_while(is_alphanumeric);
            }
        }
    }

    fn symbol(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(byte) = self.peek() {
            let opens_comment = byte == b'/' && self.peek_at(1) == Some(b'*');
            if !is_symbol(byte) || opens_comment {
                break;
            }
            self.bump();
        }
        &self.src[start..self.pos]
    }

    fn at_end_boundary(&self) -> bool {
        self.peek()
            .is_none_or(|byte| byte.is_ascii_whitespace() || byte == b'%')
    }

    // --- cursor ------------------------------------------------------------

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.pos + ahead).copied()
    }

    fn bump(&mut self) {
        if self.bytes[self.pos] == b'\n' {
            self.line += 1;
            self.line_start = self.pos + 1;
        }
        self.pos += 1;
    }

    fn bump_char(&mut self) {
        let len = self.src[self.pos..]
            .chars()
            .next()
            .map_or(1, char::len_utf8);
        for _ in 0..len {
            self.bump();
        }
    }

    fn eat(&mut self, byte: u8) {
        if self.peek() == Some(byte) {
            self.bump();
        }
    }

    fn eat_while(&mut self, predicate: impl Fn(u8) -> bool) {
        while self.peek().is_some_and(&predicate) {
            self.bump();
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            offset: self.pos,
            line: self.line,
            line_start: self.line_start,
        }
    }
}

impl Iterator for Lexer<'_> {
    type Item = Lexeme;

    fn next(&mut self) -> Option<Lexeme> {
        self.eat_while(|b| b.is_ascii_whitespace());
        let byte = self.peek()?;
        let start = self.mark();
        let kind = self.token(byte);
        Some(Lexeme {
            kind,
            start,
            end: self.mark(),
        })
    }
}

fn is_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

fn is_symbol(byte: u8) -> bool {
    matches!(
        byte,
        b'+' | b'-'
            | b'*'
            | b'/'
            | b'\\'
            | b'^'
            | b'<'
            | b'>'
            | b'='
            | b'~'
            | b':'
            | b'.'
            | b'?'
            | b'@'
            | b'#'
            | b'&'
            | b'$'
    )
}

fn is_utf8_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

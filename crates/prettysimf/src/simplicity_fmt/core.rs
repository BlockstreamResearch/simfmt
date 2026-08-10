use std::ops::Range;

use crate::config::InnerFmtConfig;

use simplicityhl::error::Span;
use simplicityhl::lexer::{FmtToken, FmtTokens, Token, TriviaKind};

pub(super) struct Context<'a> {
    pub(super) config: &'a InnerFmtConfig,
    pub(super) source: &'a str,
    pub(super) prefix_end: usize,
    pub(super) trivia: TriviaCursor,
    pub(super) syntax: SyntaxCursor,
    decimal_literals: Vec<Span>,
    used_decimal_literals: Vec<bool>,
    type_ranges: Vec<Range<usize>>,
}

impl<'a> Context<'a> {
    /// Builds the formatter context from the original source and its formatting tokens.
    ///
    /// The context keeps source-backed indexes for trivia, semicolons, and decimal
    /// literals so later document rendering can preserve details that are not fully
    /// represented in the AST.
    pub(super) fn new(config: &'a InnerFmtConfig, source: &'a str, tokens: &FmtTokens<'_>, prefix_end: usize) -> Self {
        let decimal_literals: Vec<_> = tokens
            .iter()
            .filter_map(|(token, span)| matches!(token, FmtToken::Token(Token::DecLiteral(_))).then_some(*span))
            .collect();
        let used_decimal_literals = vec![false; decimal_literals.len()];

        Self {
            config,
            source,
            prefix_end,
            trivia: TriviaCursor::from_tokens(tokens),
            syntax: SyntaxCursor::from_tokens(tokens),
            decimal_literals,
            used_decimal_literals,
            type_ranges: Vec::new(),
        }
    }

    /// Runs `f` while `start..end` is the active source range for a type.
    ///
    /// Type formatting uses this range to recover original decimal literals for
    /// type-level constants, such as array sizes and list bounds.
    pub(super) fn exec_with_type_range<T>(&mut self, start: usize, end: usize, f: impl FnOnce(&mut Self) -> T) -> T {
        self.type_ranges.push(start..end);
        let output = f(self);
        self.type_ranges.pop();
        output
    }

    /// Returns the original decimal literal spelling for a type-level constant.
    ///
    /// The AST stores constants as values, so this searches decimal tokens inside
    /// the active type range and compares them after removing underscores. Matching
    /// tokens are marked as used so repeated equal values keep their own spelling.
    pub(super) fn original_type_decimal(&mut self, value: usize) -> Option<String> {
        let range = self.type_ranges.last()?;

        for (index, span) in self.decimal_literals.iter().enumerate() {
            if self.used_decimal_literals[index] || span.start < range.start || range.end < span.end {
                continue;
            }

            let literal = self.source.get(span.start..span.end)?;
            if decimal_literal_value(literal) == Some(value) {
                self.used_decimal_literals[index] = true;
                return Some(literal.to_owned());
            }
        }

        None
    }
}

fn decimal_literal_value(literal: &str) -> Option<usize> {
    let digits: String = literal.chars().filter(|character| *character != '_').collect();
    digits.parse().ok()
}

#[derive(Clone, Debug)]
pub(super) struct Trivia {
    pub(super) kind: TriviaKind,
    pub(super) span: Span,
}

impl Trivia {
    pub(super) fn is_comment(&self) -> bool {
        matches!(self.kind, TriviaKind::LineComment | TriviaKind::BlockComment)
    }

    pub(super) fn is_line_comment(&self) -> bool {
        matches!(self.kind, TriviaKind::LineComment)
    }

    pub(super) fn is_block_comment(&self) -> bool {
        matches!(self.kind, TriviaKind::BlockComment)
    }

    pub(super) fn is_newline(&self) -> bool {
        matches!(self.kind, TriviaKind::Newline)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SyntaxKind {
    Arrow,
    Eq,
    FatArrow,
    Comma,
    Semi,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
}

impl SyntaxKind {
    const COUNT: usize = 11;

    const fn index(self) -> usize {
        match self {
            Self::Arrow => 0,
            Self::Eq => 1,
            Self::FatArrow => 2,
            Self::Comma => 3,
            Self::Semi => 4,
            Self::LParen => 5,
            Self::RParen => 6,
            Self::LBracket => 7,
            Self::RBracket => 8,
            Self::LBrace => 9,
            Self::RBrace => 10,
        }
    }
}

/// Punctuation spans from the same lossless token stream as formatter trivia.
///
/// These boundaries let document builders attach comments around delimiters
/// without searching or editing raw source text.
#[derive(Debug)]
pub(super) struct SyntaxCursor {
    tokens: [Vec<Span>; SyntaxKind::COUNT],
}

impl SyntaxCursor {
    pub(super) fn from_tokens(tokens: &FmtTokens<'_>) -> Self {
        let mut syntax_tokens = std::array::from_fn(|_| Vec::new());

        for (token, span) in tokens {
            let FmtToken::Token(token) = token else {
                continue;
            };
            let kind = match token {
                Token::Arrow => SyntaxKind::Arrow,
                Token::Eq => SyntaxKind::Eq,
                Token::FatArrow => SyntaxKind::FatArrow,
                Token::Comma => SyntaxKind::Comma,
                Token::Semi => SyntaxKind::Semi,
                Token::LParen => SyntaxKind::LParen,
                Token::RParen => SyntaxKind::RParen,
                Token::LBracket => SyntaxKind::LBracket,
                Token::RBracket => SyntaxKind::RBracket,
                Token::LBrace => SyntaxKind::LBrace,
                Token::RBrace => SyntaxKind::RBrace,
                _ => continue,
            };
            syntax_tokens[kind.index()].push(*span);
        }

        Self { tokens: syntax_tokens }
    }

    pub(super) fn first_in(&self, kind: SyntaxKind, start: usize, end: usize) -> Option<&Span> {
        let spans = &self.tokens[kind.index()];
        spans
            .get(spans.partition_point(|span| span.start < start))
            .filter(|span| span.end <= end)
    }

    pub(super) fn last_in(&self, kind: SyntaxKind, start: usize, end: usize) -> Option<&Span> {
        let spans = &self.tokens[kind.index()];
        spans[..spans.partition_point(|span| span.end <= end)]
            .last()
            .filter(|span| span.start >= start)
    }
}

/// Ordered, lossless lexer trivia owned by the formatter.
///
/// Trivia is removed only when the formatter renders the corresponding source
/// range. This lets a parent consume its inter-child gaps without taking trivia
/// belonging to a nested AST node.
#[derive(Debug)]
pub(super) struct TriviaCursor {
    trivia: Vec<Trivia>,
    consumed: Vec<bool>,
}

impl TriviaCursor {
    pub(super) fn from_tokens(tokens: &FmtTokens<'_>) -> Self {
        let trivia: Vec<Trivia> = tokens
            .iter()
            .filter_map(|(token, span)| match token {
                FmtToken::Trivia(trivia) => Some(Trivia {
                    kind: trivia.kind(),
                    span: *span,
                }),
                FmtToken::Token(_) => None,
                _ => unreachable!(),
            })
            .collect();

        let consumed = vec![false; trivia.len()];

        Self { trivia, consumed }
    }

    pub(super) fn has_comment_in(&self, start: usize, end: usize) -> bool {
        self.indices_in(start, end)
            .any(|index| !self.consumed[index] && self.trivia[index].is_comment())
    }

    pub(super) fn take_gap(&mut self, start: usize, end: usize) -> Vec<Trivia> {
        let mut gap = Vec::new();
        for index in self.indices_in(start, end) {
            if !self.consumed[index] {
                self.consumed[index] = true;
                gap.push(self.trivia[index].clone());
            }
        }
        gap
    }

    pub(super) fn remaining_comments(&self) -> impl Iterator<Item = &Trivia> {
        self.trivia
            .iter()
            .zip(&self.consumed)
            .filter_map(|(trivia, consumed)| (!consumed && trivia.is_comment()).then_some(trivia))
    }

    fn indices_in(&self, start: usize, end: usize) -> std::ops::Range<usize> {
        let first = self.trivia.partition_point(|trivia| trivia.span.start < start);
        let last = first + self.trivia[first..].partition_point(|trivia| trivia.span.end <= end);
        first..last
    }
}

#[cfg(test)]
mod tests {
    use super::{Context, SyntaxCursor, SyntaxKind, Trivia, TriviaCursor};
    use crate::config::InnerFmtConfig;
    use simplicityhl::error::Span;
    use simplicityhl::lexer::{FmtToken, Token, TriviaKind};

    fn span(start: usize, end: usize) -> Span {
        Span::new(0, start..end)
    }

    #[test]
    fn finds_syntax_tokens_by_kind_and_source_range() {
        let tokens = vec![
            (FmtToken::Token(Token::LBrace), span(1, 2)),
            (FmtToken::Token(Token::Comma), span(3, 4)),
            (FmtToken::Token(Token::Comma), span(5, 6)),
            (FmtToken::Token(Token::RBrace), span(7, 8)),
        ];
        let cursor = SyntaxCursor::from_tokens(&tokens);

        assert_eq!(cursor.first_in(SyntaxKind::Comma, 2, 7), Some(&span(3, 4)));
        assert_eq!(cursor.last_in(SyntaxKind::Comma, 2, 7), Some(&span(5, 6)));
        assert_eq!(cursor.first_in(SyntaxKind::Comma, 4, 5), None);
        assert_eq!(cursor.last_in(SyntaxKind::Comma, 6, 7), None);
    }

    #[test]
    fn takes_only_requested_trivia_even_when_ranges_arrive_out_of_order() {
        let mut cursor = TriviaCursor {
            trivia: vec![
                Trivia {
                    kind: TriviaKind::Whitespace,
                    span: span(1, 2),
                },
                Trivia {
                    kind: TriviaKind::LineComment,
                    span: span(5, 7),
                },
                Trivia {
                    kind: TriviaKind::Newline,
                    span: span(8, 9),
                },
            ],
            consumed: vec![false; 3],
        };

        assert!(cursor.has_comment_in(3, 8));
        assert_eq!(cursor.take_gap(5, 9).len(), 2);
        assert!(!cursor.has_comment_in(3, 8));
        assert_eq!(cursor.take_gap(0, 3).len(), 1);
        assert!(cursor.remaining_comments().next().is_none());
    }
}

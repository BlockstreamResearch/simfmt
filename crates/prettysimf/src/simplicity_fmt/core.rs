use std::ops::Range;

use crate::config::InnerFmtConfig;

use simplicityhl::error::Span;
use simplicityhl::lexer::{FmtToken, FmtTokens, Token, TriviaKind};

pub struct Context<'a> {
    pub config: &'a InnerFmtConfig,
    pub source: &'a str,
    pub prefix_end: usize,
    pub trivia: TriviaCursor,
    semicolons: Vec<Span>,
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
    pub fn new(config: &'a InnerFmtConfig, source: &'a str, tokens: &FmtTokens<'_>, prefix_end: usize) -> Self {
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
            semicolons: tokens
                .iter()
                .filter_map(|(token, span)| matches!(token, FmtToken::Token(Token::Semi)).then_some(*span))
                .collect(),
            decimal_literals,
            used_decimal_literals,
            type_ranges: Vec::new(),
        }
    }

    /// Returns the end offset of the first semicolon between two source positions.
    ///
    /// This is used when formatting adjacent statements: the AST spans end before
    /// the separating semicolon, while gap formatting needs to start after it.
    pub fn semicolon_end_between(&self, start: usize, end: usize) -> Option<usize> {
        self.semicolons
            .get(self.semicolons.partition_point(|span| span.start < start))
            .filter(|span| span.end <= end)
            .map(|span| span.end)
    }

    /// Runs `f` while `start..end` is the active source range for a type.
    ///
    /// Type formatting uses this range to recover original decimal literals for
    /// type-level constants, such as array sizes and list bounds.
    pub fn exec_with_type_range<T>(&mut self, start: usize, end: usize, f: impl FnOnce(&mut Self) -> T) -> T {
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
    pub fn original_type_decimal(&mut self, value: usize) -> Option<String> {
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
pub struct Trivia {
    pub kind: TriviaKind,
    pub span: Span,
}

impl Trivia {
    pub fn is_comment(&self) -> bool {
        matches!(self.kind, TriviaKind::LineComment | TriviaKind::BlockComment)
    }

    pub fn is_newline(&self) -> bool {
        matches!(self.kind, TriviaKind::Newline)
    }
}

/// Ordered, lossless lexer trivia owned by the formatter.
///
/// Trivia is removed only when the formatter renders the corresponding source
/// range. This lets a parent consume its inter-child gaps without taking trivia
/// belonging to a nested AST node.
#[derive(Debug)]
pub struct TriviaCursor {
    trivia: Vec<Trivia>,
    consumed: Vec<bool>,
}

impl TriviaCursor {
    pub fn from_tokens(tokens: &FmtTokens<'_>) -> Self {
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

    pub fn has_comment_in(&self, start: usize, end: usize) -> bool {
        self.indices_in(start, end)
            .any(|index| !self.consumed[index] && self.trivia[index].is_comment())
    }

    pub fn take_gap(&mut self, start: usize, end: usize) -> Vec<Trivia> {
        let mut gap = Vec::new();
        for index in self.indices_in(start, end) {
            if !self.consumed[index] {
                self.consumed[index] = true;
                gap.push(self.trivia[index].clone());
            }
        }
        gap
    }

    pub fn remaining_comments(&self) -> impl Iterator<Item = &Trivia> {
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
    use super::{Context, Trivia, TriviaCursor};
    use crate::config::InnerFmtConfig;
    use simplicityhl::error::Span;
    use simplicityhl::lexer::TriviaKind;

    fn span(start: usize, end: usize) -> Span {
        Span::new(0, start..end)
    }

    #[test]
    fn finds_semicolons_without_scanning_the_prefix() {
        let config = InnerFmtConfig::default();
        let context = Context {
            config: &config,
            source: "",
            prefix_end: 0,
            trivia: TriviaCursor {
                trivia: Vec::new(),
                consumed: Vec::new(),
            },
            semicolons: vec![span(2, 3), span(10, 11), span(20, 21)],
            decimal_literals: Vec::new(),
            used_decimal_literals: Vec::new(),
            type_ranges: Vec::new(),
        };

        assert_eq!(context.semicolon_end_between(4, 15), Some(11));
        assert_eq!(context.semicolon_end_between(4, 10), None);
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

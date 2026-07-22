use crate::config::InnerFmtConfig;

use simplicityhl::error::Span;
use simplicityhl::lexer::{FmtToken, FmtTokens, Token, TriviaKind};

pub struct Context<'a> {
    pub config: &'a InnerFmtConfig,
    pub source: &'a str,
    pub prefix_end: usize,
    pub trivia: TriviaCursor,
    semicolons: Vec<Span>,
}

impl<'a> Context<'a> {
    pub fn new(config: &'a InnerFmtConfig, source: &'a str, tokens: &FmtTokens<'_>, prefix_end: usize) -> Self {
        Self {
            config,
            source,
            prefix_end,
            trivia: TriviaCursor::from_tokens(tokens),
            semicolons: tokens
                .iter()
                .filter_map(|(token, span)| matches!(token, FmtToken::Token(Token::Semi)).then_some(*span))
                .collect(),
        }
    }

    pub fn semicolon_end_between(&self, start: usize, end: usize) -> Option<usize> {
        self.semicolons
            .iter()
            .find(|span| span.start >= start && span.end <= end)
            .map(|span| span.end)
    }
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
}

impl TriviaCursor {
    pub fn from_tokens(tokens: &FmtTokens<'_>) -> Self {
        let trivia = tokens
            .iter()
            .filter_map(|(token, span)| match token {
                FmtToken::Trivia(kind) => Some(Trivia {
                    kind: kind.clone(),
                    span: span.clone(),
                }),
                FmtToken::Token(_) => None,
            })
            .collect();

        Self { trivia }
    }

    pub fn has_comment_in(&self, start: usize, end: usize) -> bool {
        self.trivia
            .iter()
            .any(|trivia| trivia.is_comment() && trivia.span.start >= start && trivia.span.end <= end)
    }

    pub fn take_gap(&mut self, start: usize, end: usize) -> Vec<Trivia> {
        let mut gap = Vec::new();
        self.trivia.retain(|trivia| {
            if trivia.span.start >= start && trivia.span.end <= end {
                gap.push(trivia.clone());
                false
            } else {
                true
            }
        });
        gap
    }

    pub fn remaining_comments(&self) -> impl Iterator<Item = &Trivia> {
        self.trivia.iter().filter(|trivia| trivia.is_comment())
    }
}

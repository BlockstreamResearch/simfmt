use crate::config::InnerFmtConfig;
use crate::simplicity_fmt::comments::TriviaCursor;
use simplicityhl::lexer::FmtTokens;

pub struct Context<'a> {
    pub config: &'a InnerFmtConfig,
    pub source: &'a str,
    pub prefix_end: usize,
    pub trivia: TriviaCursor,
}

impl<'a> Context<'a> {
    pub fn new(config: &'a InnerFmtConfig, source: &'a str, tokens: &FmtTokens<'_>, prefix_end: usize) -> Self {
        Self {
            config,
            source,
            prefix_end,
            trivia: TriviaCursor::from_tokens(tokens),
        }
    }
}

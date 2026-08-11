use std::string::FromUtf8Error;

use crate::config::{Color, FmtConfig, NewlineStyle};
use crate::fmt_processor::FormatterSession;
use crate::utils::{EmitMode, FormatInput, Verbosity};

/// User-facing formatting options for in-memory source.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct FormatOptions {
    /// Number of spaces per indentation level.
    pub indent_width: usize,
    /// Preferred maximum width of each line.
    pub line_width: usize,
    /// Line endings to use in formatted output.
    pub newline_style: NewlineStyle,
}

impl Default for FormatOptions {
    fn default() -> Self {
        let fmt_conf = FmtConfig::default();

        Self {
            indent_width: fmt_conf.indent_width(),
            line_width: fmt_conf.line_width(),
            newline_style: fmt_conf.newline_style(),
        }
    }
}

impl FormatOptions {
    fn to_fmt_config(self) -> FmtConfig {
        let mut conf = FmtConfig::default();
        // user-facing config
        conf.set().indent_width(self.indent_width);
        conf.set().line_width(self.line_width);
        conf.set().newline_style(self.newline_style);

        // inner config
        conf.set().verbose(Verbosity::Quiet);
        conf.set().emit_mode(EmitMode::Stdout);
        conf.set().color(Color::Never);
        conf.set().print_misformatted_file_names(false);

        conf
    }
}

/// Error returned by [`pretty_simf_please`].
#[derive(thiserror::Error, Debug)]
pub enum PrettySimfError {
    /// Formatting could not start or complete an operational step.
    #[error("Formatting operation failed: {0}")]
    Operational(String),
    /// Source was parsed but could not be formatted safely.
    #[error("Formatting failed:\n{0}")]
    FormatError(String),
    /// Formatted bytes unexpectedly contained invalid UTF-8.
    #[error("String conversion error: '{0}'")]
    StringConversion(#[from] FromUtf8Error),
}

/// Formats SimplicityHL source held in memory and returns the formatted source.
pub fn pretty_simf_please(input: impl Into<String>, options: FormatOptions) -> Result<String, PrettySimfError> {
    let input = input.into();
    let config = options.to_fmt_config();
    let mut buf = Vec::with_capacity(input.len() * 2);

    let fmt_result = {
        let mut session = FormatterSession::new(config, Some(&mut buf));
        session.format(FormatInput::Text(input))
    };

    match fmt_result {
        Ok(report) if report.has_warnings() => Err(PrettySimfError::FormatError(report.to_string())),
        Ok(_) => Ok(String::from_utf8(buf)?),
        Err(error) => Err(PrettySimfError::Operational(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_source_text_without_writing_a_file() {
        let input = include_str!("../../cli/tests/source/real_contracts/single_bit.simf");
        let expected = include_str!("../../cli/tests/target/real_contracts/single_bit.simf");

        let formatted = pretty_simf_please(input.to_owned(), FormatOptions::default()).unwrap();

        assert_eq!(formatted, expected);
    }
}

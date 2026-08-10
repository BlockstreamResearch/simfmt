use crate::config::{Color, FmtConfig, NewlineStyle};
use crate::error::ErrorKind;
use crate::fmt_processor::Session;
use crate::reporter::FormatReport;
use crate::utils::{EmitMode, Input, Verbosity};
use std::string::FromUtf8Error;

#[derive(Debug, Clone, Copy)]
pub struct FmtFriendlyConfig {
    /// Number of spaces per tab
    pub indent_width: usize,
    /// Maximum width of each line
    pub line_width: usize,
    /// Unix or Windows line endings
    pub newline_style: NewlineStyle,
}

impl Default for FmtFriendlyConfig {
    fn default() -> Self {
        let fmt_conf = FmtConfig::default();

        Self {
            indent_width: fmt_conf.indent_width(),
            line_width: fmt_conf.line_width(),
            newline_style: fmt_conf.newline_style(),
        }
    }
}

impl FmtFriendlyConfig {
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

#[derive(thiserror::Error, Debug)]
pub enum PrettySimfError {
    #[error("Formatting operation failed: {0}")]
    Operational(ErrorKind),
    #[error("Formatting failed:\n{0}")]
    FormatError(FormatReport),
    #[error("String conversion error: '{0}'")]
    StringConversion(#[from] FromUtf8Error),
}

/// Formats SimplicityHL source held in memory and returns the formatted source.
pub fn pretty_simf_please(input: String, conf: FmtFriendlyConfig) -> Result<String, PrettySimfError> {
    let conf = conf.to_fmt_config();
    let mut buf = Vec::with_capacity(input.len() * 2);

    let fmt_result = {
        let mut session = Session::new(conf, Some(&mut buf));
        session.format(Input::Text(input))
    };

    match fmt_result {
        Ok(report) if report.has_warnings() => Err(PrettySimfError::FormatError(report)),
        Ok(_) => Ok(String::from_utf8(buf)?),
        Err(error) => Err(PrettySimfError::Operational(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_source_text_without_writing_a_file() {
        let input = include_str!("../../cli/tests/source/real_contracts/single_bit.simf");
        let expected = include_str!("../../cli/tests/target/real_contracts/single_bit.simf");

        let formatted = pretty_simf_please(input.to_owned(), FmtFriendlyConfig::default()).unwrap();

        assert_eq!(formatted, expected);
    }
}

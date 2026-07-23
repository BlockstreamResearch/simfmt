use crate::config::{FmtConfig, InnerFmtConfig, NewlineStyle};
use crate::error::ErrorKind;
use crate::fmt_processor::{FormatHandler, FormattingError};
use crate::newline_style::apply_newline_style;
use crate::reporter::FormatReport;
use crate::utils::{FileName, Input};
use simplicityhl::error::Diagnostic;
use simplicityhl::parse::{ParsedSource, Program};
use std::sync::Arc;

pub struct FormatContext<'a, T: FormatHandler> {
    pub report: FormatReport,
    fmt_config: &'a FmtConfig,
    handler: &'a mut T,
}

pub struct RawFormatContext {
    file: FileName,
    input_text: Arc<str>,
    buffer: String,
}

pub type ParsedProgram<'src> = Result<ParsedSource<'src>, Vec<Diagnostic>>;

impl RawFormatContext {
    pub fn new(input: Input) -> Result<Self, ErrorKind> {
        let main_file = input.file_name();
        let text = input.load()?;
        let buffer = String::with_capacity(text.len() * 2);
        let input_text: Arc<str> = Arc::from(text);

        Ok(RawFormatContext {
            file: main_file,
            input_text,
            buffer,
        })
    }

    pub fn format_lines(&mut self, fmt_config: &InnerFmtConfig) {
        let parsed = Program::parse_for_formatting(0, self.input_text.as_ref(), &simplicityhl::UnstableFeatures::all());

        match parsed {
            Ok(parsed) => {
                match crate::simplicity_fmt::fmt::format_program(&parsed, self.input_text.as_ref(), &fmt_config) {
                    Ok(formatted) => self.buffer.push_str(&formatted),
                    Err(_) => self.buffer.push_str(self.input_text.as_ref()),
                }
            }
            Err(_) => self.buffer.push_str(self.input_text.as_ref()),
        }
    }
}

impl<'a, T: FormatHandler + 'a> FormatContext<'a, T> {
    pub fn new(report: FormatReport, config: &'a FmtConfig, handler: &'a mut T) -> Self {
        FormatContext {
            report,
            fmt_config: config,
            handler,
        }
    }

    // Formats a single file.
    pub fn format_file(&mut self, mut raw_ctx: RawFormatContext) -> Result<(), ErrorKind> {
        let formatting_config = self.fmt_config.formatting_config();
        raw_ctx.format_lines(&formatting_config);

        ensure_single_trailing_newline(
            self.fmt_config.newline_style(),
            &mut raw_ctx.buffer,
            raw_ctx.input_text.as_ref(),
        );

        apply_newline_style(
            self.fmt_config.newline_style(),
            &mut raw_ctx.buffer,
            raw_ctx.input_text.as_ref(),
        );

        self.handler
            .handle_formatted_file(raw_ctx.file, raw_ctx.input_text, raw_ctx.buffer, &mut self.report)
    }
}

fn ensure_single_trailing_newline(newline_style: NewlineStyle, formatted_text: &mut String, raw_input_text: &str) {
    if formatted_text.is_empty() {
        return;
    }

    let content_len = formatted_text.trim_end_matches(['\r', '\n']).len();
    formatted_text.truncate(content_len);
    formatted_text.push_str("\n");
    apply_newline_style(newline_style, formatted_text, raw_input_text);
}

fn simplicity_err_to_fmt_err(simplicity_errs: Vec<Diagnostic>) -> Vec<FormattingError> {
    simplicity_errs
        .into_iter()
        .map(FormattingError::from_simplicity_err)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensures_exactly_one_trailing_newline() {
        let mut formatted = String::from("fn main() {}\n\n\n");
        ensure_single_trailing_newline(NewlineStyle::Unix, &mut formatted, "");

        assert_eq!(formatted, "fn main() {}\n");
    }

    #[test]
    fn uses_the_auto_detected_newline_style() {
        let mut formatted = String::from("fn main() {}");
        ensure_single_trailing_newline(NewlineStyle::Auto, &mut formatted, "fn main() {}\r\n");

        assert_eq!(formatted, "fn main() {}\r\n");
    }

    #[test]
    fn leaves_empty_formatted_text_empty() {
        let mut formatted = String::new();
        ensure_single_trailing_newline(NewlineStyle::Unix, &mut formatted, "");

        assert!(formatted.is_empty());
    }
}

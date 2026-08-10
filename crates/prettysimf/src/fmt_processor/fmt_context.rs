use std::sync::Arc;

use crate::config::{FmtConfig, InnerFmtConfig, NewlineStyle};
use crate::error::ErrorKind;
use crate::fmt_processor::{FormatHandler, FormattingError};
use crate::newline_style::apply_newline_style;
use crate::reporter::FormatReport;
use crate::utils::{FileName, FormatInput};

use simplicityhl::error::{Diagnostic, DiagnosticManager};
use simplicityhl::parse::{ParsedSource, Program};

pub(crate) struct FormatContext<'a, T: FormatHandler> {
    pub(crate) report: FormatReport,
    fmt_config: &'a FmtConfig,
    handler: &'a mut T,
}

pub(crate) struct RawFormatContext {
    file: FileName,
    input_text: Arc<str>,
    buffer: String,
}

#[allow(dead_code)]
pub(crate) type ParsedProgram<'src> = Result<ParsedSource<'src>, Vec<Diagnostic>>;

impl RawFormatContext {
    pub(crate) fn new(input: FormatInput) -> Result<Self, ErrorKind> {
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

    pub(crate) fn format_lines(&mut self, fmt_config: &InnerFmtConfig) -> Result<(), Vec<FormattingError>> {
        let mut diagnostics_manager = DiagnosticManager::default();
        let parsed = Program::parse_with_errors_for_fmt(
            0,
            self.input_text.as_ref(),
            &simplicityhl::UnstableFeatures::all(),
            &mut diagnostics_manager,
        );

        if diagnostics_manager.has_errors() {
            let errors = diagnostics_manager.diagnostics();
            return Err(simplicity_err_to_fmt_err(errors, self.input_text.as_ref()));
        }

        let parsed =
            parsed.ok_or_else(|| [FormattingError::from_error_kind(ErrorKind::ParseError, "Empty program")])?;

        let formatted = crate::simplicity_fmt::format_program(&parsed, self.input_text.as_ref(), fmt_config)
            .map_err(|error| vec![FormattingError::from_error_kind(error, self.input_text.as_ref())])?;
        self.buffer.push_str(&formatted);

        Ok(())
    }
}

impl<'a, T: FormatHandler + 'a> FormatContext<'a, T> {
    pub(crate) fn new(report: FormatReport, config: &'a FmtConfig, handler: &'a mut T) -> Self {
        FormatContext {
            report,
            fmt_config: config,
            handler,
        }
    }

    // Formats a single file.
    pub(crate) fn format_file(&mut self, mut raw_ctx: RawFormatContext) -> Result<(), ErrorKind> {
        let formatting_config = self.fmt_config.formatting_config();
        if let Err(errors) = raw_ctx.format_lines(&formatting_config) {
            let has_parsing_errors = errors.iter().any(|error| matches!(&error.kind, ErrorKind::ParseError));
            self.report.append(raw_ctx.file.clone(), errors);
            if has_parsing_errors {
                self.report.add_parsing_error();
            }
            return Ok(());
        }

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
    formatted_text.push('\n');
    apply_newline_style(newline_style, formatted_text, raw_input_text);
}

fn simplicity_err_to_fmt_err(simplicity_errs: &[Diagnostic], source: &str) -> Vec<FormattingError> {
    simplicity_errs
        .iter()
        .map(|diagnostic| FormattingError::from_simplicity_err(diagnostic, source))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reporter::FormatReportFormatterBuilder;

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

    #[test]
    fn parser_errors_are_rendered_with_source_context() {
        let source = "// before one\n// before two\nfn main() {\n    match true {\n        false => () true => (),\n    }\n}\n// after one\n// after two\n";
        let mut diagnostics = DiagnosticManager::default();
        let _program =
            Program::parse_with_errors_for_fmt(0, source, &simplicityhl::UnstableFeatures::all(), &mut diagnostics);

        assert!(diagnostics.has_errors(), "missing match-arm comma must not parse");
        let mut report = FormatReport::new();
        report.append(
            FileName::Stdin,
            simplicity_err_to_fmt_err(diagnostics.diagnostics(), source),
        );
        report.add_parsing_error();

        let rendered = FormatReportFormatterBuilder::new(&report).build().to_string();

        assert!(rendered.contains("error: Grammar error: Missing ',' after a match arm"));
        assert!(rendered.contains("--> <stdin>:5:"), "{rendered}");
        assert!(rendered.contains("false => () true =>"));
        assert!(rendered.contains('^'));
        assert!(!rendered.contains("// before one"), "{rendered}");
        assert!(!rendered.contains("// after one"), "{rendered}");
    }

    #[test]
    fn lost_comments_are_rendered_as_user_errors() {
        let source = "// missed comment\nfn main() {}\n";
        let report = FormatReport::new();
        report.append(
            FileName::Stdin,
            vec![FormattingError::from_error_kind(
                ErrorKind::LostComment {
                    line: 1,
                    start: 0,
                    end: 17,
                },
                source,
            )],
        );

        let rendered = FormatReportFormatterBuilder::new(&report).build().to_string();

        assert!(rendered.contains("error: cannot format comment on line 1 at byte span 0..17"));
        assert!(rendered.contains("--> <stdin>:1:1"));
        assert!(rendered.contains("// missed comment"));
        assert!(!rendered.contains("internal"));
        assert!(report.internal.borrow().1.has_unformatted_code_errors);
    }
}

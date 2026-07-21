use crate::config::{FmtConfig, InnerFmtConfig};
use crate::error::ErrorKind;
use crate::fmt_processor::{FormatHandler, FormattingError};
use crate::format_report_formatter::FormatReport;
use crate::newline_style::apply_newline_style;
use crate::utils::{FileName, Input};
use simplicityhl::error::RichError;
use simplicityhl::parse::{ParsedSource, Program};
use simplicityhl::source::SourceFile;
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

pub type ParsedProgram<'src> = Result<ParsedSource<'src>, Vec<RichError>>;

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
        let source = SourceFile::anonymous(self.input_text.clone());
        let parsed = Program::parse_for_formatting(0, &source, &simplicityhl::UnstableFeatures::none());

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

        apply_newline_style(
            self.fmt_config.newline_style(),
            &mut raw_ctx.buffer,
            raw_ctx.input_text.as_ref(),
        );

        self.handler
            .handle_formatted_file(raw_ctx.file, raw_ctx.input_text, raw_ctx.buffer, &mut self.report)
    }
}

fn simplicity_err_to_fmt_err(simplicity_errs: Vec<RichError>) -> Vec<FormattingError> {
    simplicity_errs
        .into_iter()
        .map(FormattingError::from_simplicity_err)
        .collect()
}

use std::io;
use std::io::Write;
use std::sync::Arc;

use crate::config::{FmtConfig, PartialConfig};
use crate::emitter::Emitter;
use crate::error::ErrorKind;
use crate::fmt_processor::{FormatContext, FormatHandler, RawFormatContext, ReportedErrors};
use crate::reporter::{FormatReport, FormatReportFormatterBuilder};
use crate::source_file;
use crate::utils::{FileName, FormatInput, Timer, should_emit_verbose};

/// A session is a run of simfmt across single or multiple inputs.
pub struct FormatterSession<'b, T: Write> {
    config: FmtConfig,
    out: Option<&'b mut T>,
    errors: ReportedErrors,
    emitter: Box<dyn Emitter + 'b>,
}

pub(crate) fn format_project<T: FormatHandler>(
    input: FormatInput,
    config: &FmtConfig,
    handler: &mut T,
) -> Result<FormatReport, ErrorKind> {
    let mut timer = Timer::start();

    let report = FormatReport::new();
    let main_file = input.file_name();
    let input_is_stdin = main_file == FileName::Stdin;
    // TODO: decouple parsing stage somehow to make correct logs

    timer = timer.done_parsing();

    // Parse the Simplicity program.
    let mut context = FormatContext::new(report, config, handler);
    let raw_ctx = RawFormatContext::new(input)?;

    // Format file
    should_emit_verbose(input_is_stdin, config, || println!("Formatting {}", main_file));
    context.format_file(raw_ctx)?;

    timer = timer.done_formatting();

    should_emit_verbose(input_is_stdin, config, || {
        println!(
            "Spent {0:.3} secs in the parsing phase, and {1:.3} secs in the formatting phase",
            timer.get_parse_time(),
            timer.get_format_time(),
        )
    });

    Ok(context.report)
}

impl<'b, T: Write + 'b> FormatterSession<'b, T> {
    pub(crate) fn format_input_inner(&mut self, input: FormatInput) -> Result<FormatReport, ErrorKind> {
        let config = self.config.clone();
        let format_result = format_project(input, &config, self);
        self.config.record_used_options(&config.used_options());

        format_result.inspect(|report| {
            self.errors.add(&report.internal.borrow().1);
        })
    }
}

impl<'b, T: Write + 'b> FormatHandler for FormatterSession<'b, T> {
    // Called for each formatted file.
    fn handle_formatted_file(
        &mut self,
        path: FileName,
        orig_file: Arc<str>,
        result: String,
        report: &mut FormatReport,
    ) -> Result<(), ErrorKind> {
        if let Some(ref mut out) = self.out {
            match source_file::write_file(
                &path,
                orig_file,
                &result,
                out,
                &mut *self.emitter,
                self.config.newline_style(),
            ) {
                Ok(ref result) if result.has_diff => report.add_diff(),
                Err(e) => {
                    // Create a new error with path_str to help users see which files failed
                    let err_msg = format!("{path}: {e}");
                    return Err(io::Error::new(e.kind(), err_msg).into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl<'b, T: Write + 'b> FormatterSession<'b, T> {
    /// Creates a formatter session with resolved configuration and optional output.
    pub fn new(config: FmtConfig, mut out: Option<&'b mut T>) -> FormatterSession<'b, T> {
        let emitter = config.emit_mode().create_emitter(&config.get_emitter_conf());

        if let Some(ref mut out) = out {
            let _ = emitter.emit_header(out);
        }

        FormatterSession {
            config,
            out,
            emitter,
            errors: ReportedErrors::default(),
        }
    }

    /// The main entry point for Rustfmt. Formats the given input according to the
    /// given config. `out` is only necessary if required by the configuration.
    pub(crate) fn format(&mut self, input: FormatInput) -> Result<FormatReport, ErrorKind> {
        self.format_input_inner(input)
    }

    /// Returns the configuration options consulted during this session.
    #[must_use]
    pub fn used_options(&self) -> PartialConfig {
        self.config.used_options()
    }

    /// Returns whether an input/output operation failed.
    #[must_use]
    fn has_operational_errors(&self) -> bool {
        self.errors.has_operational_errors
    }

    /// Returns whether parsing failed for any input.
    #[must_use]
    fn has_parsing_errors(&self) -> bool {
        self.errors.has_parsing_errors
    }

    /// Returns whether valid source could not be formatted.
    #[must_use]
    fn has_formatting_errors(&self) -> bool {
        self.errors.has_formatting_errors
    }

    /// Returns whether an opt-in formatter check failed.
    #[must_use]
    fn has_check_errors(&self) -> bool {
        self.errors.has_check_errors
    }

    /// Returns whether formatted output differs from the input.
    #[must_use]
    fn has_diff(&self) -> bool {
        self.errors.has_diff
    }

    /// Returns whether formatting could not preserve all source content.
    #[must_use]
    fn has_unformatted_code_errors(&self) -> bool {
        self.errors.has_unformatted_code_errors
    }

    /// Returns whether no detailed error category was recorded.
    #[must_use]
    pub fn has_no_errors(&self) -> bool {
        !(self.has_operational_errors()
            || self.has_parsing_errors()
            || self.has_formatting_errors()
            || self.has_check_errors()
            || self.has_diff()
            || self.has_unformatted_code_errors())
    }

    /// Formats one input, emits its report, and records detailed error state.
    pub fn format_and_emit_report(&mut self, input: FormatInput) {
        match self.format(input) {
            Ok(report) => {
                if report.has_warnings() {
                    eprintln!(
                        "{}",
                        FormatReportFormatterBuilder::new(&report)
                            .enable_colors(self.should_print_with_colors())
                            .build()
                    );
                }
            }
            Err(msg) => {
                eprintln!("Error writing files: {msg}");
                self.add_operational_error();
            }
        }
    }

    fn add_operational_error(&mut self) {
        self.errors.has_operational_errors = true;
    }

    fn should_print_with_colors(&self) -> bool {
        term::stderr().is_some_and(|t| {
            self.config.color().use_colored_tty() && t.supports_color() && t.supports_attr(term::Attr::Bold)
        })
    }
}

impl<'b, T: Write + 'b> Drop for FormatterSession<'b, T> {
    fn drop(&mut self) {
        if let Some(ref mut out) = self.out {
            let _ = self.emitter.emit_footer(out);
        }
    }
}

use crate::config::FmtConfig;
use crate::emitter::Emitter;
use crate::error::ErrorKind;
use crate::fmt_processor::{FormatContext, FormatHandler, RawFormatContext, ReportedErrors, SourceFile};
use crate::format_report_formatter::FormatReport;
use crate::source_file;
use crate::utils::{FileName, Input, Timer, should_emit_verbose};
use std::io;
use std::io::Write;
use std::sync::Arc;

/// A session is a run of rustfmt across a single or multiple inputs.
pub struct Session<'b, T: Write> {
    pub config: FmtConfig,
    pub out: Option<&'b mut T>,
    pub(crate) errors: ReportedErrors,
    source_file: SourceFile,
    emitter: Box<dyn Emitter + 'b>,
}

fn format_project<T: FormatHandler>(
    input: Input,
    config: &FmtConfig,
    handler: &mut T,
) -> Result<FormatReport, ErrorKind> {
    let mut timer = Timer::start();

    let mut report = FormatReport::new();
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

impl<'b, T: Write + 'b> Session<'b, T> {
    pub(crate) fn format_input_inner(&mut self, input: Input) -> Result<FormatReport, ErrorKind> {
        let config = self.config.clone();
        let format_result = format_project(input, &config, self);
        self.config.record_used_options(&config.used_options());

        format_result.map(|report| {
            self.errors.add(&report.internal.borrow().1);
            report
        })
    }
}

impl<'b, T: Write + 'b> FormatHandler for Session<'b, T> {
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

        self.source_file.push((path, result));
        Ok(())
    }
}

impl<'b, T: Write + 'b> Session<'b, T> {
    pub fn new(config: FmtConfig, mut out: Option<&'b mut T>) -> Session<'b, T> {
        let emitter = config.emit_mode().create_emitter(&config.get_emitter_conf());

        if let Some(ref mut out) = out {
            let _ = emitter.emit_header(out);
        }

        Session {
            config,
            out,
            emitter,
            errors: ReportedErrors::default(),
            source_file: vec![],
        }
    }

    /// The main entry point for Rustfmt. Formats the given input according to the
    /// given config. `out` is only necessary if required by the configuration.
    pub fn format(&mut self, input: Input) -> Result<FormatReport, ErrorKind> {
        self.format_input_inner(input)
    }

    pub fn add_operational_error(&mut self) {
        self.errors.has_operational_errors = true;
    }

    pub fn has_operational_errors(&self) -> bool {
        self.errors.has_operational_errors
    }

    pub fn has_parsing_errors(&self) -> bool {
        self.errors.has_parsing_errors
    }

    pub fn has_formatting_errors(&self) -> bool {
        self.errors.has_formatting_errors
    }

    pub fn has_check_errors(&self) -> bool {
        self.errors.has_check_errors
    }

    pub fn has_diff(&self) -> bool {
        self.errors.has_diff
    }

    pub fn has_unformatted_code_errors(&self) -> bool {
        self.errors.has_unformatted_code_errors
    }

    pub fn has_no_errors(&self) -> bool {
        !(self.has_operational_errors()
            || self.has_parsing_errors()
            || self.has_formatting_errors()
            || self.has_check_errors()
            || self.has_diff()
            || self.has_unformatted_code_errors()
            || self.errors.has_macro_format_failure)
    }
}

impl<'b, T: Write + 'b> Drop for Session<'b, T> {
    fn drop(&mut self) {
        if let Some(ref mut out) = self.out {
            let _ = self.emitter.emit_footer(out);
        }
    }
}

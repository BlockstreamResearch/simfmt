use annotate_snippets::{Annotation, AnnotationKind, Group, Level, Renderer, Snippet};

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::error::ErrorKind;
use crate::fmt_processor::{FormatErrorMap, FormattingError, ReportedErrors};
use crate::utils::FileName;

/// A builder for [`FormatReportFormatter`].
pub struct FormatReportFormatterBuilder<'a> {
    report: &'a FormatReport,
    enable_colors: bool,
}

impl<'a> FormatReportFormatterBuilder<'a> {
    /// Creates a new [`FormatReportFormatterBuilder`].
    pub fn new(report: &'a FormatReport) -> Self {
        Self {
            report,
            enable_colors: false,
        }
    }

    /// Enables colors and formatting in the output.
    #[must_use]
    pub fn enable_colors(self, enable_colors: bool) -> Self {
        Self { enable_colors, ..self }
    }

    /// Creates a new [`FormatReportFormatter`] from the settings in this builder.
    pub fn build(self) -> FormatReportFormatter<'a> {
        FormatReportFormatter {
            report: self.report,
            enable_colors: self.enable_colors,
        }
    }
}

/// Formats the warnings/errors in a [`FormatReport`].
///
/// Can be created using a [`FormatReportFormatterBuilder`].
pub struct FormatReportFormatter<'a> {
    report: &'a FormatReport,
    enable_colors: bool,
}

impl<'a> fmt::Display for FormatReportFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let errors_by_file = &self.report.internal.borrow().0;

        let renderer = if self.enable_colors {
            Renderer::styled()
        } else {
            Renderer::plain()
        };

        for (file, errors) in errors_by_file {
            for error in errors {
                let error_kind = error.kind.to_string();

                let mut title = error_kind_to_snippet_annotation_level(&error.kind).primary_title(&error_kind);

                if error.is_internal() {
                    title = title.id("internal");
                }

                let mut group = Group::with_title(title);

                let message_suffix = error.msg_suffix();
                if !message_suffix.is_empty() {
                    group = group.element(Level::NOTE.message(message_suffix));
                }

                let origin = format!("{}:{}", file, error.line);
                let snippet = Snippet::source(&error.line_buffer)
                    .line_start(error.line)
                    .path(&origin)
                    .fold(false)
                    .annotations(annotation(error));

                group = group.element(snippet);

                let report = [group];
                writeln!(f, "{}\n", renderer.render(&report))?;
            }
        }

        if !errors_by_file.is_empty() {
            let label = format!(
                "simplefmt has failed to format. See previous {} errors.",
                self.report.warning_count()
            );

            let title = Level::WARNING.primary_title(&label);
            let group = Group::with_title(title);
            let report = [group];

            writeln!(f, "{}", renderer.render(&report))?;
        }

        Ok(())
    }
}

fn annotation(error: &FormattingError) -> Option<Annotation<'_>> {
    let (range_start, range_length) = error.format_len();
    let range_end = range_start + range_length;

    if range_length > 0 {
        Some(AnnotationKind::Context.span(range_start..range_end))
    } else {
        None
    }
}

fn error_kind_to_snippet_annotation_level(error_kind: &ErrorKind) -> Level {
    match error_kind {
        ErrorKind::LineOverflow(..)
        | ErrorKind::TrailingWhitespace
        | ErrorKind::IoError(_)
        | ErrorKind::ParseError
        | ErrorKind::LostComment
        | ErrorKind::BadAttr
        | ErrorKind::VersionMismatch => Level::ERROR,
        ErrorKind::DeprecatedAttr => Level::WARNING,
    }
}

#[derive(Clone)]
pub struct FormatReport {
    // Maps stringified file paths to their associated formatting errors.
    pub(crate) internal: Rc<RefCell<(FormatErrorMap, ReportedErrors)>>,
    non_formatted_ranges: Vec<(usize, usize)>,
}

impl FormatReport {
    pub fn new() -> FormatReport {
        FormatReport {
            internal: Rc::new(RefCell::new((HashMap::new(), ReportedErrors::default()))),
            non_formatted_ranges: Vec::new(),
        }
    }

    pub fn add_non_formatted_ranges(&mut self, mut ranges: Vec<(usize, usize)>) {
        self.non_formatted_ranges.append(&mut ranges);
    }

    pub fn append(&self, f: FileName, mut v: Vec<FormattingError>) {
        self.track_errors(&v);
        self.internal
            .borrow_mut()
            .0
            .entry(f)
            .and_modify(|fe| fe.append(&mut v))
            .or_insert(v);
    }

    pub fn track_errors(&self, new_errors: &[FormattingError]) {
        let errs = &mut self.internal.borrow_mut().1;
        if !new_errors.is_empty() {
            errs.has_formatting_errors = true;
        }
        if errs.has_operational_errors && errs.has_check_errors && errs.has_unformatted_code_errors {
            return;
        }
        for err in new_errors {
            match err.kind {
                ErrorKind::LineOverflow(..) => {
                    errs.has_operational_errors = true;
                }
                ErrorKind::TrailingWhitespace => {
                    errs.has_operational_errors = true;
                    errs.has_unformatted_code_errors = true;
                }
                ErrorKind::LostComment => {
                    errs.has_unformatted_code_errors = true;
                }
                ErrorKind::DeprecatedAttr | ErrorKind::BadAttr | ErrorKind::VersionMismatch => {
                    errs.has_check_errors = true;
                }
                _ => {}
            }
        }
    }

    pub fn add_diff(&mut self) {
        self.internal.borrow_mut().1.has_diff = true;
    }

    pub fn add_parsing_error(&mut self) {
        self.internal.borrow_mut().1.has_parsing_errors = true;
    }

    pub(crate) fn warning_count(&self) -> usize {
        self.internal.borrow().0.values().map(|errors| errors.len()).sum()
    }

    /// Whether any warnings or errors are present in the report.
    pub fn has_warnings(&self) -> bool {
        self.internal.borrow().1.has_formatting_errors
    }
}

impl fmt::Display for FormatReport {
    // Prints all the formatting errors.
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(fmt, "{}", FormatReportFormatterBuilder::new(self).build())?;
        Ok(())
    }
}

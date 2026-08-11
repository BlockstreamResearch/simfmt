use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::error::ErrorKind;
use crate::fmt_processor::{FormatErrorMap, FormattingError, ReportedErrors};
use crate::utils::FileName;

use annotate_snippets::{Annotation, AnnotationKind, Group, Level, Renderer, Snippet};

/// A builder for [`FormatReportFormatter`].
pub(crate) struct FormatReportFormatterBuilder<'a> {
    report: &'a FormatReport,
    enable_colors: bool,
}

impl<'a> FormatReportFormatterBuilder<'a> {
    /// Creates a new [`FormatReportFormatterBuilder`].
    pub(crate) fn new(report: &'a FormatReport) -> Self {
        Self {
            report,
            enable_colors: false,
        }
    }

    /// Enables colors and formatting in the output.
    #[must_use]
    pub(crate) fn enable_colors(self, enable_colors: bool) -> Self {
        Self { enable_colors, ..self }
    }

    /// Creates a new [`FormatReportFormatter`] from the settings in this builder.
    pub(crate) fn build(self) -> FormatReportFormatter<'a> {
        FormatReportFormatter {
            report: self.report,
            enable_colors: self.enable_colors,
        }
    }
}

/// Formats the warnings/errors in a [`FormatReport`].
///
/// Can be created using a [`FormatReportFormatterBuilder`].
pub(crate) struct FormatReportFormatter<'a> {
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
                let message = error.message.as_deref().unwrap_or(&error_kind);

                let mut title = error_kind_to_snippet_annotation_level(&error.kind).primary_title(message);

                if error.is_internal() {
                    title = title.id("internal");
                }

                let mut group = Group::with_title(title);

                if error.should_render_snippet() {
                    let snippet = Snippet::source(&error.line_buffer)
                        .path(file.to_string())
                        .annotations(annotation(error));

                    group = group.element(snippet);
                }

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
    error.annotation_range().map(|range| {
        let kind = if matches!(error.kind, ErrorKind::ParseError) {
            AnnotationKind::Primary
        } else {
            AnnotationKind::Context
        };
        kind.span(range)
    })
}

fn error_kind_to_snippet_annotation_level(error_kind: &ErrorKind) -> Level<'_> {
    match error_kind {
        ErrorKind::IoError(_)
        | ErrorKind::ParseError
        | ErrorKind::LostComment { .. }
        | ErrorKind::FailedToBuildDocument
        | ErrorKind::FailedToRenderDocument
        | ErrorKind::InvalidFormattedOutput(_) => Level::ERROR,
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FormatReport {
    // Maps stringified file paths to their associated formatting errors.
    pub(crate) internal: Rc<RefCell<(FormatErrorMap, ReportedErrors)>>,
}

impl FormatReport {
    pub(crate) fn new() -> FormatReport {
        FormatReport {
            internal: Rc::new(RefCell::new((HashMap::new(), ReportedErrors::default()))),
        }
    }

    pub(crate) fn append(&self, f: FileName, mut v: Vec<FormattingError>) {
        self.track_errors(&v);
        self.internal
            .borrow_mut()
            .0
            .entry(f)
            .and_modify(|fe| fe.append(&mut v))
            .or_insert(v);
    }

    pub(crate) fn track_errors(&self, new_errors: &[FormattingError]) {
        let errs = &mut self.internal.borrow_mut().1;
        if !new_errors.is_empty() {
            errs.has_formatting_errors = true;
        }
        if errs.has_unformatted_code_errors {
            return;
        }
        for err in new_errors {
            if let ErrorKind::LostComment { .. } = err.kind {
                errs.has_unformatted_code_errors = true;
            }
        }
    }

    pub(crate) fn add_diff(&mut self) {
        self.internal.borrow_mut().1.has_diff = true;
    }

    pub(crate) fn add_parsing_error(&mut self) {
        self.internal.borrow_mut().1.has_parsing_errors = true;
    }

    pub(crate) fn warning_count(&self) -> usize {
        self.internal.borrow().0.values().map(|errors| errors.len()).sum()
    }

    /// Whether any warnings or errors are present in the report.
    pub(crate) fn has_warnings(&self) -> bool {
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

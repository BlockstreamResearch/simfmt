use std::ops::Range;

use crate::error::ErrorKind;

use simplicityhl::error::{Diagnostic, Location};

#[derive(Debug)]
pub(crate) struct FormattingError {
    pub(crate) kind: ErrorKind,
    pub(crate) line_buffer: String,
    pub(crate) message: Option<String>,
    pub(crate) span: Option<Range<usize>>,
}

impl FormattingError {
    fn clamped_span(source: &str, span: Range<usize>) -> Range<usize> {
        let start = span.start.min(source.len());
        let end = span.end.min(source.len()).max(start);
        start..end
    }

    pub(crate) fn from_simplicity_err(simplicity_err: &Diagnostic, source: &str) -> FormattingError {
        let span = match simplicity_err.location() {
            Location::Code(span) => Some(Self::clamped_span(source, span.start..span.end)),
            Location::File(_) | Location::Global => None,
        };

        FormattingError {
            kind: ErrorKind::ParseError,
            line_buffer: source.to_owned(),
            message: Some(simplicity_err.to_string()),
            span,
        }
    }

    pub(crate) fn from_error_kind(kind: ErrorKind, source: &str) -> FormattingError {
        let span = match &kind {
            ErrorKind::LostComment { start, end, .. } => Some(Self::clamped_span(source, *start..*end)),
            ErrorKind::IoError(_)
            | ErrorKind::ParseError
            | ErrorKind::FailedToBuildDocument
            | ErrorKind::FailedToRenderDocument
            | ErrorKind::InvalidFormattedOutput(_) => None,
        };

        FormattingError {
            kind,
            line_buffer: source.to_owned(),
            message: None,
            span,
        }
    }

    pub(crate) fn is_internal(&self) -> bool {
        matches!(self.kind, ErrorKind::IoError(_))
    }

    pub(crate) fn annotation_range(&self) -> Option<Range<usize>> {
        self.span.clone()
    }

    pub(crate) fn should_render_snippet(&self) -> bool {
        self.span.is_some()
    }
}

#[derive(Default, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct ReportedErrors {
    // Encountered e.g., an IO error.
    pub(crate) has_operational_errors: bool,

    // Failed to reformat code because of parsing errors.
    pub(crate) has_parsing_errors: bool,

    // Code is valid, but it is impossible to format it properly.
    pub(crate) has_formatting_errors: bool,

    // Failed an opt-in checking.
    pub(crate) has_check_errors: bool,

    /// Formatted code differs from existing code (--check only).
    pub(crate) has_diff: bool,

    /// Formatted code missed something, like lost comments or extra trailing space
    pub(crate) has_unformatted_code_errors: bool,
}

impl ReportedErrors {
    /// Combine two summaries together.
    pub(crate) fn add(&mut self, other: &ReportedErrors) {
        self.has_operational_errors |= other.has_operational_errors;
        self.has_parsing_errors |= other.has_parsing_errors;
        self.has_formatting_errors |= other.has_formatting_errors;
        self.has_check_errors |= other.has_check_errors;
        self.has_diff |= other.has_diff;
        self.has_unformatted_code_errors |= other.has_unformatted_code_errors;
    }
}

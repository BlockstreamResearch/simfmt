use crate::error::ErrorKind;
use simplicityhl::error::{RichError, Span};

pub(crate) struct FormattingError {
    pub(crate) line: usize,
    pub(crate) kind: ErrorKind,
    is_comment: bool,
    is_string: bool,
    pub(crate) line_buffer: String,
}

impl FormattingError {
    // pub(crate) fn from_span(span: Span, psess: &ParseSess, kind: ErrorKind) -> FormattingError {
    //     FormattingError {
    //         line: todo!(), //psess.line_of_byte_pos(span.start),
    //         is_comment: kind.is_comment(),
    //         kind,
    //         is_string: false,
    //         line_buffer: todo!(), // psess.span_to_first_line_string(span),
    //     }
    // }

    pub(crate) fn from_simplicity_err(simplicity_err: RichError) -> FormattingError {
        FormattingError {
            line: simplicity_err.span().start,
            is_comment: false,
            kind: ErrorKind::ParseError,
            is_string: false,
            line_buffer: simplicity_err.span().to_string(),
        }
    }

    pub(crate) fn is_internal(&self) -> bool {
        match self.kind {
            ErrorKind::LineOverflow(..)
            | ErrorKind::TrailingWhitespace
            | ErrorKind::IoError(_)
            | ErrorKind::ParseError
            | ErrorKind::LostComment => true,
            _ => false,
        }
    }

    pub(crate) fn msg_suffix(&self) -> &str {
        if self.is_comment || self.is_string {
            "set `error_on_unformatted = false` to suppress \
             the warning against comments or string literals\n"
        } else {
            ""
        }
    }

    // (space, target)
    pub(crate) fn format_len(&self) -> (usize, usize) {
        match self.kind {
            ErrorKind::LineOverflow(found, max) => (max, found - max),
            ErrorKind::TrailingWhitespace | ErrorKind::DeprecatedAttr | ErrorKind::BadAttr | ErrorKind::LostComment => {
                let trailing_ws_start = self
                    .line_buffer
                    .rfind(|c: char| !c.is_whitespace())
                    .map(|pos| pos + 1)
                    .unwrap_or(0);
                (trailing_ws_start, self.line_buffer.len() - trailing_ws_start)
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub(crate) struct ReportedErrors {
    // Encountered e.g., an IO error.
    pub(crate) has_operational_errors: bool,

    // Failed to reformat code because of parsing errors.
    pub(crate) has_parsing_errors: bool,

    // Code is valid, but it is impossible to format it properly.
    pub(crate) has_formatting_errors: bool,

    // Code contains macro call that was unable to format.
    pub(crate) has_macro_format_failure: bool,

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
        self.has_macro_format_failure |= other.has_macro_format_failure;
        self.has_check_errors |= other.has_check_errors;
        self.has_diff |= other.has_diff;
        self.has_unformatted_code_errors |= other.has_unformatted_code_errors;
    }
}

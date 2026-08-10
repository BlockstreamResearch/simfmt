use std::io::Error as IoError;
use std::string::FromUtf8Error;

use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum ErrorKind {
    /// An io error during reading or writing.
    #[error("io error: {0}")]
    IoError(#[from] IoError),
    /// Parse error occurred when parsing the input.
    #[error("parse error")]
    ParseError,
    /// A comment could not be assigned to a formatting document because the
    /// surrounding semantic syntax does not expose usable source spans.
    #[error(
        "cannot format comment on line {line} at byte span {start}..{end}: \
         surrounding syntax has no usable source span"
    )]
    LostComment { line: usize, start: usize, end: usize },
    /// The formatter could not construct a document from valid syntax.
    #[error("formatter failed to construct a document")]
    FailedToBuildDocument,
    /// The formatter could not render its document.
    #[error("formatter failed to render a document")]
    FailedToRenderDocument,
    /// The formatter emitted invalid UTF-8.
    #[error("formatter produced invalid UTF-8: {0}")]
    InvalidFormattedOutput(FromUtf8Error),
}

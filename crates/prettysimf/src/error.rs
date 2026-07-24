use std::io::Error as IoError;
use std::ops::Range;
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ErrorKind {
    /// An io error during reading or writing.
    #[error("io error: {0}")]
    IoError(#[from] IoError),
    /// Parse error occurred when parsing the input.
    #[error("parse error")]
    ParseError,
    /// If we had formatted the given node, then we would have lost a comment.
    #[error("not formatted because a comment would be lost")]
    LostComment(Range<usize>),
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

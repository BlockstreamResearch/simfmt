use globwalk::GlobError;
use std::io;
use std::io::Error as IoError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Glob error: {0}")]
    Glob(#[from] GlobError),
    #[error("Invalid generation path: '{0}'")]
    GenerationPath(String),
    #[error("Failed to extract content from path, err: '{0}'")]
    FailedToExtractContent(io::Error),
    #[error("Failed to generate file: {0}")]
    GenerationFailed(String),
    #[error(
        "Failed to resolve correct relative path for include_simf! macro, cwd: '{cwd:?}', simf_file: '{simf_file:?}'"
    )]
    FailedToFindCorrectRelativePath { cwd: PathBuf, simf_file: PathBuf },
    #[error("Failed to find prefix for a file: {0}")]
    NoBasePathForGeneration(#[from] std::path::StripPrefixError),
    #[error("Dependency '{dep_name}' is missing its configuration manifest at: {expected_path}")]
    MissingDependencyConfig { dep_name: String, expected_path: PathBuf },
    #[error("{0}")]
    PathCanonicalization(String),
    #[error("Failed to build dependency map: {0}")]
    DependencyMap(String),
    #[error("Failed to flatten program: {0}")]
    Flattening(String),
    #[error("Invalid git repository URL: '{0}'")]
    InvalidGitUrl(String),
}

/// Rustfmt operations errors.
#[derive(Error, Debug)]
pub enum OperationError {
    /// An unknown help topic was requested.
    #[error("Unknown help topic: `{0}`.")]
    UnknownHelpTopic(String),
    /// An io error during reading or writing.
    #[error("{0}")]
    IoError(IoError),
}

#[derive(Error, Debug)]
pub enum ErrorKind {
    /// Line has exceeded character limit (found, maximum).
    #[error(
        "line formatted, but exceeded maximum width \
         (maximum: {1} (see `max_width` option), found: {0})"
    )]
    LineOverflow(usize, usize),
    /// Line ends in whitespace.
    #[error("left behind trailing whitespace")]
    TrailingWhitespace,
    /// Used deprecated skip attribute.
    #[error("`rustfmt_skip` is deprecated; use `rustfmt::skip`")]
    DeprecatedAttr,
    /// Used a rustfmt:: attribute other than skip or skip::macros.
    #[error("invalid attribute")]
    BadAttr,
    /// An io error during reading or writing.
    #[error("io error: {0}")]
    IoError(io::Error),
    /// Parse error occurred when parsing the input.
    #[error("parse error")]
    ParseError,
    /// The user mandated a version and the current version of Rustfmt does not
    /// satisfy that requirement.
    #[error("version mismatch")]
    VersionMismatch,
    /// If we had formatted the given node, then we would have lost a comment.
    #[error("not formatted because a comment would be lost")]
    LostComment,
}

impl ErrorKind {
    pub fn is_comment(&self) -> bool {
        matches!(self, ErrorKind::LostComment)
    }
}

impl From<io::Error> for ErrorKind {
    fn from(e: io::Error) -> ErrorKind {
        ErrorKind::IoError(e)
    }
}

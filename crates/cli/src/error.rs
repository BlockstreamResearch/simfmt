use std::io::Error as IoError;

use prettysimf::driver::EmitMode;
use thiserror::Error;

/// Simfmt operations errors.
#[derive(Error, Debug)]
pub(crate) enum OperationError {
    /// An unknown help topic was requested.
    #[error("Unknown help topic: `{0}`.")]
    UnknownHelpTopic(String),
    /// An unknown print-config option was requested.
    #[error("Unknown print-config option: `{0}`.")]
    UnknownPrintConfigTopic(String),
    /// Attempt to generate a minimal config from standard input.
    #[error("The `--print-config=minimal` option doesn't work with standard input.")]
    MinimalPathWithStdin,
    /// An io error during reading or writing.
    #[error("{0}")]
    IoError(IoError),
    /// Attempt to use --emit with a mode which is not currently
    /// supported with standard input.
    #[error("Emit mode {0} not supported with standard output.")]
    StdinBadEmit(EmitMode),
}

impl From<IoError> for OperationError {
    fn from(e: IoError) -> OperationError {
        OperationError::IoError(e)
    }
}

use crate::error::ErrorKind;
use crate::fmt_processor::FormattingError;
use crate::utils::FileName;

use crate::format_report_formatter::FormatReport;
use std::collections::HashMap;
use std::sync::Arc;
// todo: add module resolution logic

pub(crate) type SourceFile = Vec<FileRecord>;
pub(crate) type FileRecord = (FileName, String);
pub(crate) type FormatErrorMap = HashMap<FileName, Vec<FormattingError>>;

// Handle the results of formatting.
pub trait FormatHandler {
    fn handle_formatted_file(
        &mut self,
        path: FileName,
        orig_file: Arc<str>,
        result: String,
        report: &mut FormatReport,
    ) -> Result<(), ErrorKind>;
}

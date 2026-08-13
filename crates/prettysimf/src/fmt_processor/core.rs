use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ErrorKind;
use crate::fmt_processor::FormattingError;
use crate::reporter::FormatReport;
use crate::utils::FileName;

pub(crate) type FormatErrorMap = HashMap<FileName, Vec<FormattingError>>;

// Handle the results of formatting.
pub(crate) trait FormatHandler {
    fn handle_formatted_file(
        &mut self,
        path: FileName,
        orig_file: Arc<str>,
        result: String,
        report: &mut FormatReport,
    ) -> Result<(), ErrorKind>;
}

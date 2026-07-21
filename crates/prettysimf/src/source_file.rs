use crate::emitter::{self, Emitter};
use crate::utils::FileName;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

// Append a newline to the end of each file.
pub(crate) fn append_newline(s: &mut String) {
    s.push('\n');
}

use crate::config::NewlineStyle;

pub(crate) fn write_file<T>(
    filename: &FileName,
    orig_file: Arc<str>,
    formatted_text: &str,
    out: &mut T,
    emitter: &mut dyn Emitter,
    newline_style: NewlineStyle,
) -> Result<emitter::EmitterResult, io::Error>
where
    T: Write,
{
    fn ensure_real_path(filename: &FileName) -> &Path {
        match *filename {
            FileName::Real(ref path) => path,
            _ => panic!("cannot format `{filename}` and emit to files"),
        }
    }

    let original_text = if newline_style != NewlineStyle::Auto && *filename != FileName::Stdin {
        Arc::from(fs::read_to_string(ensure_real_path(filename))?)
    } else {
        orig_file
    };

    let formatted_file = emitter::FormattedFile {
        filename,
        original_text: original_text.as_ref(),
        formatted_text,
    };

    emitter.emit_formatted_file(out, formatted_file)
}

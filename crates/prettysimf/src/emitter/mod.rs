pub(crate) use self::diff::*;
pub(crate) use self::files::*;
pub(crate) use self::stdout::*;
use crate::config::Color;
use crate::utils::{FileName, Verbosity};
use std::io::{self, Write};
use std::path::Path;

mod diff;
mod files;
mod stdout;

pub(crate) struct FormattedFile<'a> {
    pub(crate) filename: &'a FileName,
    pub(crate) original_text: &'a str,
    pub(crate) formatted_text: &'a str,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct EmitterResult {
    pub(crate) has_diff: bool,
}

pub(crate) trait Emitter {
    fn emit_formatted_file(
        &mut self,
        output: &mut dyn Write,
        formatted_file: FormattedFile<'_>,
    ) -> Result<EmitterResult, io::Error>;

    fn emit_header(&self, _output: &mut dyn Write) -> Result<(), io::Error> {
        Ok(())
    }

    fn emit_footer(&self, _output: &mut dyn Write) -> Result<(), io::Error> {
        Ok(())
    }
}

fn ensure_real_path(filename: &FileName) -> &Path {
    match *filename {
        FileName::Real(ref path) => path,
        _ => panic!("cannot format `{filename}` and emit to files"),
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmitterConfig {
    /// Responsible for printing values in console in `check` mode
    pub print_misformatted_file_names: bool,
    pub verbose: Verbosity,
    pub color: Color,
}

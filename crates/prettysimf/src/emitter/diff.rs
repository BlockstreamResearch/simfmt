use super::*;
use crate::utils::{OutputWriter, Verbosity};
use similar::{ChangeTag, TextDiff};

pub(crate) struct DiffEmitter {
    config: EmitterConfig,
}

impl DiffEmitter {
    pub(crate) fn new(config: EmitterConfig) -> Self {
        Self { config }
    }
}

impl Emitter for DiffEmitter {
    fn emit_formatted_file(
        &mut self,
        output: &mut dyn Write,
        FormattedFile {
            filename,
            original_text,
            formatted_text,
        }: FormattedFile<'_>,
    ) -> Result<EmitterResult, io::Error> {
        const CONTEXT_SIZE: usize = 3;
        let mismatch = make_diff(original_text, formatted_text, CONTEXT_SIZE);
        let has_diff = !mismatch.newline_terminated();

        if has_diff {
            if self.config.print_misformatted_file_names {
                writeln!(output, "{filename}")?;
            } else {
                print_diff(
                    mismatch,
                    |line_num| match line_num {
                        None => {
                            format!("Diff in {}:", filename)
                        }
                        Some(line_num) => {
                            format!("Diff in {}:{}:", filename, line_num)
                        }
                    },
                    &self.config,
                );
            }
        } else if original_text != formatted_text {
            // This occurs when the only difference between the original and formatted values
            // is the newline style. This happens because The make_diff function compares the
            // original and formatted values line by line, independent of line endings.
            writeln!(output, "Incorrect newline style in {filename}")?;
            return Ok(EmitterResult { has_diff: true });
        }

        Ok(EmitterResult { has_diff })
    }
}

fn print_diff<F>(diff: TextDiff<str>, get_section_title: F, config: &EmitterConfig)
where
    F: Fn(Option<usize>) -> String,
{
    let line_terminator = if config.verbose == Verbosity::Verbose {
        "⏎"
    } else {
        ""
    };

    for mismatch in diff.iter_all_changes() {
        let title = get_section_title(mismatch.old_index());
        OutputWriter::writeln(&title);

        let str = mismatch.value();
        match mismatch.tag() {
            ChangeTag::Equal => OutputWriter::writeln(&format!(" {str}{line_terminator}")),
            ChangeTag::Delete => OutputWriter::writeln(&format!("-{str}{line_terminator}")),
            ChangeTag::Insert => OutputWriter::writeln(&format!("+{str}{line_terminator}")),
        }
    }
}

fn make_diff<'a>(original: &'a str, formatted: &'a str, context_size: usize) -> TextDiff<'a, 'a, str> {
    let diff = similar::TextDiff::from_lines(original, formatted);
    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn does_not_print_when_no_files_reformatted() {
        let mut writer = Vec::new();
        let config = EmitterConfig::default();
        let mut emitter = DiffEmitter::new(config);
        let result = emitter
            .emit_formatted_file(
                &mut writer,
                FormattedFile {
                    filename: &FileName::Real(PathBuf::from("src/lib.rs")),
                    original_text: "fn empty() {}\n",
                    formatted_text: "fn empty() {}\n",
                },
            )
            .unwrap();
        assert_eq!(result.has_diff, false);
        assert_eq!(writer.len(), 0);
    }

    #[test]
    fn prints_file_names_when_config_is_enabled() {
        let bin_file = "src/bin.rs";
        let bin_original = "fn main() {\nprintln!(\"Hello, world!\");\n}";
        let bin_formatted = "fn main() {\n    println!(\"Hello, world!\");\n}";
        let lib_file = "src/lib.rs";
        let lib_original = "fn greet() {\nprintln!(\"Greetings!\");\n}";
        let lib_formatted = "fn greet() {\n    println!(\"Greetings!\");\n}";

        let mut writer = Vec::new();
        let mut config = EmitterConfig::default();
        config.print_misformatted_file_names = true;
        let mut emitter = DiffEmitter::new(config);
        let _ = emitter
            .emit_formatted_file(
                &mut writer,
                FormattedFile {
                    filename: &FileName::Real(PathBuf::from(bin_file)),
                    original_text: bin_original,
                    formatted_text: bin_formatted,
                },
            )
            .unwrap();
        let _ = emitter
            .emit_formatted_file(
                &mut writer,
                FormattedFile {
                    filename: &FileName::Real(PathBuf::from(lib_file)),
                    original_text: lib_original,
                    formatted_text: lib_formatted,
                },
            )
            .unwrap();

        assert_eq!(String::from_utf8(writer).unwrap(), format!("{bin_file}\n{lib_file}\n"),)
    }

    #[test]
    fn prints_newline_message_with_only_newline_style_diff() {
        let mut writer = Vec::new();
        let config = EmitterConfig::default();
        let mut emitter = DiffEmitter::new(config);
        let _ = emitter
            .emit_formatted_file(
                &mut writer,
                FormattedFile {
                    filename: &FileName::Real(PathBuf::from("src/lib.rs")),
                    original_text: "fn empty() {}\n",
                    formatted_text: "fn empty() {}\r\n",
                },
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(writer).unwrap(),
            String::from("Incorrect newline style in src/lib.rs\n")
        );
    }
}

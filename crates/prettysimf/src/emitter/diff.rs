use super::*;

use crate::utils::{OutputWriter, Verbosity};

use similar::{ChangeTag, DiffTag, TextDiff};

const MISSING_NEWLINE_HINT: &str = r#"\ <No final newline>"#;

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
        let mismatch = make_diff(original_text, formatted_text);
        let has_diff = mismatch.ops().iter().any(|op| op.tag() != DiffTag::Equal);

        if has_diff {
            if self.config.print_misformatted_file_names {
                writeln!(output, "{filename}")?;
            } else {
                print_diff(
                    mismatch,
                    |line_num| match line_num {
                        None => {
                            format!("Diff in {}: ", filename)
                        }
                        Some(line_num) => {
                            format!("Diff in {}:{}:", filename, line_num)
                        }
                    },
                    &self.config,
                );
            }
        }

        Ok(EmitterResult { has_diff })
    }
}

fn print_diff<F>(diff: TextDiff<'_, '_, str>, get_section_title: F, config: &EmitterConfig)
where
    F: Fn(Option<usize>) -> String,
{
    let is_verbose = matches!(config.verbose, Verbosity::Verbose);

    let mut writer = OutputWriter::new(config.color);
    writer.writeln(&get_section_title(None), None);

    for mismatch in diff.iter_all_changes() {
        let (prefix, color) = match mismatch.tag() {
            ChangeTag::Equal => (" ", None),
            ChangeTag::Delete => ("-", Some(term::color::RED)),
            ChangeTag::Insert => ("+", Some(term::color::GREEN)),
        };
        let line = strip_line_terminator(mismatch.value());
        let terminator = if is_verbose && !mismatch.missing_newline() {
            "⏎"
        } else {
            ""
        };

        writer.write(&format!("{prefix}{line}{terminator}"), color);

        if diff.newline_terminated() && mismatch.missing_newline() {
            writer.write(MISSING_NEWLINE_HINT, None);
        }

        writer.writeln("", None);
    }
}

fn strip_line_terminator(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .or_else(|| line.strip_suffix('\r'))
        .unwrap_or(line)
}

fn make_diff<'a>(original: &'a str, formatted: &'a str) -> TextDiff<'a, 'a, str> {
    similar::TextDiff::from_lines(original, formatted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn prints_a_single_header_per_file() {
        let header_calls = AtomicUsize::new(0);

        print_diff(
            TextDiff::from_lines("one\ntwo\n", "one\nthree\n"),
            |_| {
                header_calls.fetch_add(1, Ordering::Relaxed);
                "Diff in test.simf:".to_owned()
            },
            &EmitterConfig::default(),
        );

        assert_eq!(header_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn removes_only_the_line_terminator() {
        assert_eq!(strip_line_terminator("text\n"), "text");
        assert_eq!(strip_line_terminator("text\r\n"), "text");
        assert_eq!(strip_line_terminator("text\r"), "text");
        assert_eq!(strip_line_terminator("text  "), "text  ");
    }

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
        assert!(!result.has_diff);
        assert_eq!(writer.len(), 0);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
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
    fn detects_newline_style_diff() {
        let mut writer = Vec::new();
        let config = EmitterConfig::default();
        let mut emitter = DiffEmitter::new(config);
        let result = emitter
            .emit_formatted_file(
                &mut writer,
                FormattedFile {
                    filename: &FileName::Real(PathBuf::from("src/lib.rs")),
                    original_text: "fn empty() {}\n",
                    formatted_text: "fn empty() {}\r\n",
                },
            )
            .unwrap();
        assert!(result.has_diff);
        assert!(writer.is_empty());
    }

    #[test]
    fn identifies_a_missing_final_newline() {
        let diff = TextDiff::from_lines("last line", "replacement\n");

        assert!(diff.newline_terminated());
        assert!(diff.iter_all_changes().any(|change| change.missing_newline()));
        assert_eq!(MISSING_NEWLINE_HINT, r#"\ <No final newline>"#);
    }
}

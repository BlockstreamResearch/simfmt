use crate::config::load_config;
use crate::error::OperationError;
use anyhow::format_err;
use getopts::{Matches, Options};
use prettysimf::config::{Color, FmtConfig, PartialConfig};
use prettysimf::fmt_processor::Session;
use prettysimf::utils::{EmitMode, Input, Verbosity};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write, stdout};
use std::path::{Path, PathBuf};
use std::{env, io};

/// Simfmt operations.
enum Operation {
    /// Format files and their child modules.
    Format {
        files: Vec<PathBuf>,
        minimal_config_path: Option<String>,
    },
    /// Print the help message.
    Help(HelpOp),
    /// Print version information
    Version,
    /// Output default config to a file, or stdout if None
    ConfigOutputDefault { path: Option<String> },
    /// Output current config (as if formatting to a file) to stdout
    ConfigOutputCurrent { path: Option<String> },
    /// No file specified, read from stdin
    Stdin { input: String },
}

/// Arguments to `--help`
enum HelpOp {
    None,
    Config,
}

pub fn make_opts() -> Options {
    let mut opts = Options::new();

    opts.optopt("", "emit", "What data to emit and how", "[files|stdout]");
    opts.optopt(
        "",
        "config-path",
        "Recursively searches the given path for the simfmt.toml config file. If not \
         found reverts to the input file path",
        "[Path for the configuration file]",
    );
    opts.optopt("", "color", "Use colored output (if supported)", "[always|never|auto]");
    opts.optopt(
        "",
        "print-config",
        "Dumps a default, current, or minimal config to PATH. A minimal config is the \
         subset of the current config file used for formatting the current program. \
         `current` writes to stdout current config as if formatting the file at PATH.",
        "[default|current|minimal] PATH",
    );
    opts.optflag(
        "l",
        "files-with-diff",
        "Prints the names of mismatched files that were formatted. Prints the names of \
         files that would be formatted when used with `--check` mode. ",
    );
    opts.optflag("", "check", "Run in check mode without modifying files");
    opts.optmulti(
        "",
        "config",
        "Set options from command line. These settings take priority over .simfmt.toml",
        "[key1=val1,key2=val2...]",
    );

    opts.optflag("v", "verbose", "Print verbose output");
    opts.optflag("q", "quiet", "Print less output");
    opts.optflag("V", "version", "Show version information");
    opts.optflagopt(
        "h",
        "help",
        "Show this message or help about a specific topic: `config`",
        "=TOPIC",
    );

    opts
}

/// Parsed command line options.
#[derive(Clone, Debug, Default)]
struct GetOptsOptions {
    quiet: bool,
    verbose: bool,
    config_path: Option<PathBuf>,
    inline_config: HashMap<String, String>,
    emit_mode: Option<EmitMode>,
    check: bool,
    color: Option<Color>,
    print_misformatted_file_names: bool,
}

impl GetOptsOptions {
    const EMIT_MODES: [EmitMode; 2] = [EmitMode::Files, EmitMode::Stdout];

    fn from_matches(matches: &Matches) -> anyhow::Result<GetOptsOptions> {
        let mut options = GetOptsOptions::default();
        options.verbose = matches.opt_present("verbose");
        options.quiet = matches.opt_present("quiet");
        if options.verbose && options.quiet {
            return Err(format_err!("Can't use both `--verbose` and `--quiet`"));
        }

        options.config_path = matches.opt_str("config-path").map(PathBuf::from);

        options.inline_config = matches
            .opt_strs("config")
            .iter()
            .flat_map(|config| config.split(','))
            .map(|key_val| match key_val.char_indices().find(|(_, ch)| *ch == '=') {
                Some((middle, _)) => {
                    let (key, val) = (&key_val[..middle], &key_val[middle + 1..]);
                    if !FmtConfig::is_valid_key_val(key, val) {
                        Err(format_err!("invalid key=val pair: `{}`", key_val))
                    } else {
                        Ok((key.to_string(), val.to_string()))
                    }
                }

                None => Err(format_err!(
                    "--config expects comma-separated list of key=val pairs, found `{}`",
                    key_val
                )),
            })
            .collect::<anyhow::Result<HashMap<_, _>, _>>()?;

        options.check = matches.opt_present("check");
        if let Some(ref emit_str) = matches.opt_str("emit") {
            if options.check {
                return Err(format_err!("Invalid to use `--emit` and `--check`"));
            }

            options.emit_mode = Some(emit_mode_from_emit_str(emit_str)?);
        }

        if let Some(ref color) = matches.opt_str("color") {
            match color.parse::<Color>() {
                Ok(c) => options.color = Some(c),
                _ => return Err(format_err!("Invalid color: {}", color)),
            }
        }

        options.print_misformatted_file_names = matches.opt_present("files-with-diff");

        if let Some(ref emit_mode) = options.emit_mode {
            if !GetOptsOptions::EMIT_MODES.contains(emit_mode) {
                return Err(format_err!(
                    "Invalid value for `--emit'. {emit_mode} isn't in an acceptable emit mode list.",
                ));
            }
        }

        Ok(options)
    }

    fn to_partial_config(&self) -> PartialConfig {
        let mut config = PartialConfig::default();
        if self.verbose {
            config.verbose = Some(Verbosity::Verbose);
        } else if self.quiet {
            config.verbose = Some(Verbosity::Quiet);
        }
        if self.check {
            config.emit_mode = Some(EmitMode::Diff);
        } else if let Some(emit_mode) = self.emit_mode {
            config.emit_mode = Some(emit_mode);
        }
        if let Some(color) = self.color {
            config.color = Some(color);
        }
        if self.print_misformatted_file_names {
            config.print_misformatted_file_names = Some(true);
        }

        // Inline config overrides from --config
        for (key, val) in &self.inline_config {
            let _ = config.override_value(key, val);
        }

        config
    }

    pub(crate) fn config_path(&self) -> Option<&Path> {
        self.config_path.as_deref()
    }
}

// Returned i32 is an exit code
pub fn execute(opts: &Options) -> anyhow::Result<i32> {
    let matches = opts.parse(env::args().skip(1))?;
    let options = GetOptsOptions::from_matches(&matches)?;

    match determine_operation(&matches)? {
        Operation::Help(HelpOp::None) => {
            print_usage_to_stdout(opts, "");
            Ok(0)
        }
        Operation::Help(HelpOp::Config) => {
            FmtConfig::print_docs(&mut stdout());
            Ok(0)
        }
        Operation::Version => {
            print_version();
            Ok(0)
        }
        Operation::ConfigOutputDefault { path } => {
            let toml = FmtConfig::default().all_options().to_toml()?;
            if let Some(path) = path {
                let mut file = File::create(path)?;
                file.write_all(toml.as_bytes())?;
            } else {
                io::stdout().write_all(toml.as_bytes())?;
            }
            Ok(0)
        }
        Operation::ConfigOutputCurrent { path } => {
            let path = match path {
                Some(path) => path,
                None => return Err(format_err!("PATH required for `--print-config current`")),
            };

            let file = PathBuf::from(path);
            let file = file.canonicalize().unwrap_or(file);
            let input_path = file.parent().unwrap_or(Path::new("."));

            let (config, _) = load_config(
                Some(input_path),
                options.config_path(),
                Some(options.to_partial_config()),
            )?;
            let toml = config.all_options().to_toml()?;
            io::stdout().write_all(toml.as_bytes())?;

            Ok(0)
        }
        Operation::Stdin { input } => {
            let (mut config, _) = load_config(
                Some(Path::new(".")),
                options.config_path(),
                Some(options.to_partial_config()),
            )?;

            if options.check {
                config.set().emit_mode(EmitMode::Diff);
            } else if let Some(emit_mode) = options.emit_mode {
                if emit_mode == EmitMode::Files {
                    return Err(OperationError::StdinBadEmit(emit_mode).into());
                }
            } else {
                config.set().emit_mode(EmitMode::Stdout);
            }

            format_string(input, config)
        }
        Operation::Format {
            files,
            minimal_config_path,
        } => format(files, minimal_config_path, &options),
    }
}

pub fn format_string(input: String, config: FmtConfig) -> anyhow::Result<i32> {
    let out = &mut io::stdout();
    let mut session = Session::new(config, Some(out));

    session.format_and_emit_report(Input::Text(input));

    let exit_code = if session.has_operational_errors()
        || session.has_parsing_errors()
        || session.has_diff()
        || session.has_check_errors()
    {
        1
    } else {
        0
    };
    Ok(exit_code)
}

fn format(files: Vec<PathBuf>, minimal_config_path: Option<String>, options: &GetOptsOptions) -> anyhow::Result<i32> {
    let out = &mut io::stdout();
    let cli_config = options.to_partial_config();
    let explicit_config_path = options.config_path();
    let mut minimal_config = PartialConfig::default();
    let mut has_operational_errors = false;
    let mut has_parsing_errors = false;
    let mut has_diff = false;
    let mut has_check_errors = false;

    for file in files {
        if !file.exists() {
            eprintln!("Error: file `{}` does not exist", file.display());
            has_operational_errors = true;
        } else if file.is_dir() {
            eprintln!("Error: `{}` is a directory", file.display());
            has_operational_errors = true;
        } else {
            let input_path = if explicit_config_path.is_some() {
                None
            } else {
                Some(file.parent().unwrap_or(Path::new(".")))
            };
            let (config, _) = load_config(input_path, explicit_config_path, Some(cli_config.clone()))?;

            let mut session = Session::new(config, Some(out));
            session.format_and_emit_report(Input::File(file));

            minimal_config.merge_from(&session.config.used_options());
            has_operational_errors |= session.has_operational_errors();
            has_parsing_errors |= session.has_parsing_errors();
            has_diff |= session.has_diff();
            has_check_errors |= session.has_check_errors();
        }
    }

    if let Some(path) = minimal_config_path {
        let toml = minimal_config.to_toml()?;
        let mut file = File::create(path)?;
        file.write_all(toml.as_bytes())?;
    }

    let exit_code = if has_operational_errors || has_parsing_errors || has_diff || has_check_errors {
        1
    } else {
        0
    };
    Ok(exit_code)
}

fn print_usage_to_stdout(opts: &Options, reason: &str) {
    let sep = if reason.is_empty() {
        String::new()
    } else {
        format!("{reason}\n\n")
    };
    let msg = format!("{sep}Format SimplicityHL code\n\nusage: simfmt [options] <file>...");
    println!("{}", opts.usage(&msg));
}

fn print_version() {
    let version_number = option_env!("CARGO_PKG_VERSION").unwrap_or("unknown");
    println!("simfmt {version_number}");
    // todo: maybe add discoverability of out_dir?
}

fn determine_operation(matches: &Matches) -> anyhow::Result<Operation, OperationError> {
    if matches.opt_present("h") {
        let Some(topic) = matches.opt_str("h") else {
            return Ok(Operation::Help(HelpOp::None));
        };

        return match topic.as_str() {
            "config" => Ok(Operation::Help(HelpOp::Config)),
            _ => Err(OperationError::UnknownHelpTopic(topic)),
        };
    }
    let mut free_matches = matches.free.iter().filter(|x| !x.is_empty());

    let mut minimal_config_path = None;
    if let Some(kind) = matches.opt_str("print-config") {
        let path = free_matches.next().cloned();
        match kind.as_str() {
            "default" => return Ok(Operation::ConfigOutputDefault { path }),
            "current" => return Ok(Operation::ConfigOutputCurrent { path }),
            "minimal" => {
                minimal_config_path = path;
                if minimal_config_path.is_none() {
                    eprintln!("WARNING: PATH required for `--print-config minimal`.");
                }
            }
            _ => {
                return Err(OperationError::UnknownPrintConfigTopic(kind));
            }
        }
    }

    if matches.opt_present("version") {
        return Ok(Operation::Version);
    }

    let files: Vec<_> = free_matches
        .map(|s| {
            let p = PathBuf::from(s);
            p.canonicalize().unwrap_or(p)
        })
        .collect();

    // if no file argument is supplied, read from stdin
    if files.is_empty() {
        if minimal_config_path.is_some() {
            return Err(OperationError::MinimalPathWithStdin);
        }
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;

        return Ok(Operation::Stdin { input: buffer });
    }

    Ok(Operation::Format {
        files,
        minimal_config_path,
    })
}

fn emit_mode_from_emit_str(emit_str: &str) -> anyhow::Result<EmitMode> {
    match emit_str {
        "files" => Ok(EmitMode::Files),
        "stdout" => Ok(EmitMode::Stdout),
        _ => Err(format_err!("Invalid value for `--emit`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn minimal_config_is_collected_after_formatting() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("simfmt-cli-test-{}-{timestamp}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();

        let source = directory.join("input.simf");
        let minimal_config = directory.join("minimal.toml");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/source/real_contracts/single_bit.simf"),
            &source,
        )
        .unwrap();

        let options = GetOptsOptions {
            emit_mode: Some(EmitMode::Stdout),
            ..GetOptsOptions::default()
        };
        assert_eq!(
            format(vec![source], Some(minimal_config.display().to_string()), &options).unwrap(),
            0
        );

        let toml = fs::read_to_string(&minimal_config).unwrap();
        assert!(toml.contains("indent_width"));
        assert!(toml.contains("line_width"));
        assert!(toml.contains("newline_style"));
        assert!(!toml.contains("emit_mode"));

        fs::remove_dir_all(directory).unwrap();
    }
}

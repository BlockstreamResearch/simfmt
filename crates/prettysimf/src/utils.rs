use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fmt, io};

use crate::config::{Color, FmtConfig};
use crate::emitter;
use crate::emitter::{Emitter, EmitterConfig};

use serde::{Deserialize, Serialize};
use simplicityhl::tracker::TrackerLogLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// An enumeration to represent the verbosity levels of the Simplicity execution logging.
pub enum Verbosity {
    /// No logs will be printed.
    #[default]
    Quiet = 0,
    /// The debug mode
    Verbose = 1,
}

impl std::str::FromStr for Verbosity {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quiet" => Ok(Verbosity::Quiet),
            "verbose" => Ok(Verbosity::Verbose),
            _ => Err("invalid verbosity option"),
        }
    }
}

impl Verbosity {
    /// The maximum allowed verbosity level.
    pub const MAX_VERBOSITY_LEVEL: u8 = 2;

    /// Creates a `Verbosity` instance from the number of verbosity flags provided (e.g., -v, -vv).
    #[must_use]
    pub fn new(flags: u8) -> Self {
        match flags {
            0 => Verbosity::Quiet,
            _ => Verbosity::Verbose,
        }
    }

    /// Converts the `Verbosity` level to a corresponding `TrackerLogLevel`.
    #[must_use]
    pub fn tracker_log_level(&self) -> TrackerLogLevel {
        match self {
            Verbosity::Quiet => TrackerLogLevel::None,
            Verbosity::Verbose => TrackerLogLevel::Debug,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) enum FileName {
    Real(PathBuf),
    Stdin,
}

impl fmt::Display for FileName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FileName::Real(p) => write!(f, "{}", p.display()),
            FileName::Stdin => write!(f, "<stdin>"),
        }
    }
}

/// Writes to standard output, using colors when the requested terminal supports them.
pub(crate) struct OutputWriter {
    terminal: Option<Box<term::StdoutTerminal>>,
}

impl OutputWriter {
    pub(crate) fn new(color: Color) -> Self {
        let terminal = term::stdout().filter(|terminal| color.use_colored_tty() && terminal.supports_color());
        Self { terminal }
    }

    pub(crate) fn writeln(&mut self, msg: &str, color: Option<term::color::Color>) {
        match &mut self.terminal {
            Some(terminal) => {
                if let Some(color) = color {
                    terminal.fg(color).unwrap();
                }
                writeln!(terminal, "{msg}").unwrap();
                if color.is_some() {
                    terminal.reset().unwrap();
                }
            }
            None => println!("{msg}"),
        }
    }

    pub(crate) fn write(&mut self, msg: &str, color: Option<term::color::Color>) {
        match &mut self.terminal {
            Some(terminal) => {
                if let Some(color) = color {
                    terminal.fg(color).unwrap();
                }
                write!(terminal, "{msg}").unwrap();
                if color.is_some() {
                    terminal.reset().unwrap();
                }
            }
            None => print!("{msg}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Selects how a formatter driver emits successful formatting results.
pub enum EmitMode {
    /// Emits to files.
    Files,
    /// Writes the output to stdout.
    Stdout,
    /// Checks if a diff can be generated. If so, rustfmt outputs a diff and
    /// quits with exit code 1.
    /// This option is designed to be run in CI where a non-zero exit signifies
    /// non-standard code formatting. Used for `--check`.
    Diff,
}

impl std::str::FromStr for EmitMode {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "files" => Ok(EmitMode::Files),
            "stdout" => Ok(EmitMode::Stdout),
            "diff" => Ok(EmitMode::Diff),
            _ => Err("invalid emit_mode option"),
        }
    }
}

pub(crate) fn should_emit_verbose<F>(forbid_verbose_output: bool, config: &FmtConfig, f: F)
where
    F: Fn(),
{
    if config.verbose() == Verbosity::Verbose && !forbid_verbose_output {
        f();
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Timer {
    Disabled,
    Initialized(Instant),
    DoneParsing(Instant, Instant),
    DoneFormatting(Instant, Instant, Instant),
}

impl Timer {
    pub(crate) fn start() -> Timer {
        if cfg!(target_arch = "wasm32") {
            Timer::Disabled
        } else {
            Timer::Initialized(Instant::now())
        }
    }
    pub(crate) fn done_parsing(self) -> Self {
        match self {
            Timer::Disabled => Timer::Disabled,
            Timer::Initialized(init_time) => Timer::DoneParsing(init_time, Instant::now()),
            _ => panic!("Timer can only transition to DoneParsing from Initialized state"),
        }
    }

    pub(crate) fn done_formatting(self) -> Self {
        match self {
            Timer::Disabled => Timer::Disabled,
            Timer::DoneParsing(init_time, parse_time) => Timer::DoneFormatting(init_time, parse_time, Instant::now()),
            _ => panic!("Timer can only transition to DoneFormatting from DoneParsing state"),
        }
    }

    /// Returns the time it took to parse the source files in seconds.
    pub(crate) fn get_parse_time(&self) -> f32 {
        match *self {
            Timer::Disabled => panic!("this platform cannot time execution"),
            Timer::DoneParsing(init, parse_time) | Timer::DoneFormatting(init, parse_time, _) => {
                // This should never underflow since `Instant::now()` guarantees monotonicity.
                Self::duration_to_f32(parse_time.duration_since(init))
            }
            Timer::Initialized(..) => unreachable!(),
        }
    }

    /// Returns the time it took to go from the parsed AST to the formatted output. Parsing time is
    /// not included.
    pub(crate) fn get_format_time(&self) -> f32 {
        match *self {
            Timer::Disabled => panic!("this platform cannot time execution"),
            Timer::DoneFormatting(_init, parse_time, format_time) => {
                Self::duration_to_f32(format_time.duration_since(parse_time))
            }
            Timer::DoneParsing(..) | Timer::Initialized(..) => unreachable!(),
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn duration_to_f32(d: Duration) -> f32 {
        d.as_secs() as f32 + d.subsec_nanos() as f32 / 1_000_000_000f32
    }
}

#[derive(Debug)]
/// Source accepted by a [`crate::driver::FormatterSession`].
pub enum FormatInput {
    /// Reads source from the given file path.
    File(PathBuf),
    /// Formats source already held in memory.
    Text(String),
}

impl FormatInput {
    pub(crate) fn file_name(&self) -> FileName {
        match *self {
            FormatInput::File(ref file) => FileName::Real(file.clone()),
            FormatInput::Text(..) => FileName::Stdin,
        }
    }

    fn read_file(path: &Path) -> io::Result<String> {
        /// Defines the maximum file size allowed under 4GB.
        const MAX_FILE_SIZE: u32 = u32::MAX - 1;

        let mut file = File::open(path)?;
        let size = file.metadata().map(|metadata| metadata.len()).ok().unwrap_or(0);

        if size > MAX_FILE_SIZE.into() {
            return Err(io::Error::other(format!(
                "text files larger than {MAX_FILE_SIZE} bytes are unsupported",
            )));
        }
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(contents)
    }

    pub(crate) fn load(self) -> io::Result<String> {
        match self {
            FormatInput::File(ref file) => Self::read_file(file.as_path()),
            FormatInput::Text(text) => Ok(text),
        }
    }
}

impl EmitMode {
    pub(crate) fn create_emitter<'a>(self, config: &EmitterConfig) -> Box<dyn Emitter + 'a> {
        match self {
            EmitMode::Files => Box::new(emitter::FilesEmitter::new(config.print_misformatted_file_names)),
            EmitMode::Stdout => Box::new(emitter::StdoutEmitter::new(config.verbose)),
            EmitMode::Diff => Box::new(emitter::DiffEmitter::new(config.clone())),
        }
    }
}

use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use prettysimf::driver::{FmtConfig, PartialConfig};

/// Defines acceptable names for configuration discovery
const CONFIG_FILE_NAMES: [&str; 2] = [".simfmt.toml", "simfmt.toml"];

#[derive(Debug, thiserror::Error)]
pub(crate) enum ConfigError {
    #[error("unable to find config file for path `{0}`")]
    Missing(PathBuf),
    #[error("unable to find a simfmt config file in `{0}`")]
    MissingInDirectory(PathBuf),
    #[error("failed to access `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid config `{path}`: {source}")]
    Invalid {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("config file is not a TOML table: `{0}`")]
    NotTable(PathBuf),
}

fn io_error(path: &Path, source: io::Error) -> ConfigError {
    ConfigError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".config")))
}

// Find known config file names (`.simfmt.toml`, `simfmt.toml`) in `dir`
fn get_toml_path(dir: &Path) -> Result<Option<PathBuf>, ConfigError> {
    for config_file_name in &CONFIG_FILE_NAMES {
        let config_file = dir.join(config_file_name);
        match fs::metadata(&config_file) {
            Ok(ref md) if md.is_file() => {
                return config_file
                    .canonicalize()
                    .map(Some)
                    .map_err(|source| io_error(&config_file, source));
            }
            Err(e) if !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => {
                return Err(io_error(&config_file, e));
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Resolves the config path for the input directory.
///
/// Searches for `simfmt.toml`:
/// 1. Starts from the current `dir` and recursively checking parent directories.
/// 2. Checks home directory via `HOME` env or `USERPROFILE` for file existence.
/// 3. Checks `XDG_CONFIG_HOME` env for file existence.
fn resolve_project_file(dir: &Path) -> Result<Option<PathBuf>, ConfigError> {
    let mut current = if dir.is_relative() {
        std::env::current_dir()
            .map_err(|source| io_error(dir, source))?
            .join(dir)
    } else {
        dir.to_path_buf()
    };

    current = fs::canonicalize(&current).map_err(|source| io_error(&current, source))?;

    loop {
        if let Some(path) = get_toml_path(&current)? {
            return Ok(Some(path));
        }

        if !current.pop() {
            break;
        }
    }

    if let Some(home) = home_dir()
        && let Some(path) = get_toml_path(&home)?
    {
        return Ok(Some(path));
    }

    if let Some(mut config) = config_dir() {
        config.push("simfmt");
        if let Some(path) = get_toml_path(&config)? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn resolve_explicit_config_path(path: &Path) -> Result<PathBuf, ConfigError> {
    if !path.exists() {
        return Err(ConfigError::Missing(path.to_path_buf()));
    }

    if path.is_dir() {
        get_toml_path(path)?.ok_or_else(|| ConfigError::MissingInDirectory(path.to_path_buf()))
    } else {
        path.canonicalize().map_err(|source| io_error(path, source))
    }
}

fn read_config(path: &Path) -> Result<PartialConfig, ConfigError> {
    let toml_str = fs::read_to_string(path).map_err(|source| io_error(path, source))?;

    let parsed: toml::Value = toml::from_str(&toml_str).map_err(|source| ConfigError::Invalid {
        path: path.to_path_buf(),
        source,
    })?;

    let Some(table) = parsed.as_table() else {
        return Err(ConfigError::NotTable(path.to_path_buf()));
    };

    for key in table.keys() {
        if !FmtConfig::is_valid_key(key) {
            eprintln!("Warning: Unknown configuration option `{key}`");
        }
    }

    toml::from_str(&toml_str).map_err(|source| ConfigError::Invalid {
        path: path.to_path_buf(),
        source,
    })
}

/// Loads a config using an explicit config path when supplied, otherwise searching from the input path.
/// Command line overrides are applied last.
pub(crate) fn load_config(
    input_path: Option<&Path>,
    explicit_config_path: Option<&Path>,
    options_override: Option<PartialConfig>,
) -> Result<(FmtConfig, Option<PathBuf>), ConfigError> {
    let mut final_config = FmtConfig::default();
    let mut resolved_path = None;

    let config_path = if let Some(explicit_config_path) = explicit_config_path {
        Some(resolve_explicit_config_path(explicit_config_path)?)
    } else if let Some(input_path) = input_path {
        let search_dir = if input_path.is_dir() {
            input_path
        } else {
            input_path.parent().unwrap_or(Path::new("."))
        };
        resolve_project_file(search_dir)?
    } else {
        None
    };

    if let Some(config_path) = config_path {
        final_config.apply_override(read_config(&config_path)?);
        resolved_path = Some(config_path);
    }

    if let Some(opts) = options_override {
        final_config.apply_override(opts);
    }

    Ok((final_config, resolved_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use prettysimf::NewlineStyle;
    use prettysimf::driver::{Color, EmitMode, Verbosity};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("simfmt-config-test-{}-{timestamp}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn no_paths_or_overrides_use_default_config() {
        let (config, path) = load_config(None, None, None).unwrap();

        assert_eq!(path, None);
        assert_eq!(
            config.all_options(),
            PartialConfig {
                indent_width: Some(4),
                line_width: Some(100),
                verbose: Some(Verbosity::Quiet),
                emit_mode: Some(EmitMode::Files),
                newline_style: Some(NewlineStyle::Auto),
                color: Some(Color::Auto),
                print_misformatted_file_names: Some(false),
            }
        );
    }

    #[test]
    fn discovered_config_is_merged_before_cli_overrides() {
        let directory = TestDirectory::new();
        let nested = directory.path().join("nested");
        let input_directory = nested.join("src");
        fs::create_dir_all(&input_directory).unwrap();
        let input = input_directory.join("input.simf");
        fs::write(&input, "").unwrap();
        fs::write(directory.path().join("simfmt.toml"), "indent_width = 2\n").unwrap();
        fs::write(
            nested.join(".simfmt.toml"),
            "indent_width = 6\nline_width = 80\nverbose = \"verbose\"\nemit_mode = \"stdout\"\nnewline_style = \"unix\"\n",
        )
        .unwrap();

        let cli = PartialConfig {
            line_width: Some(120),
            ..PartialConfig::default()
        };
        let (config, path) = load_config(Some(&input), None, Some(cli)).unwrap();

        assert_eq!(path, Some(nested.join(".simfmt.toml").canonicalize().unwrap()));
        assert_eq!(config.indent_width(), 6);
        assert_eq!(config.line_width(), 120);
        assert_eq!(config.verbose(), Verbosity::Verbose);
        assert_eq!(config.emit_mode(), EmitMode::Stdout);
        assert_eq!(config.newline_style(), NewlineStyle::Unix);
    }

    #[test]
    fn dotfile_takes_precedence_when_explicit_directory_contains_both_names() {
        let directory = TestDirectory::new();
        let project = directory.path().join("project");
        let explicit_directory = directory.path().join("explicit");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&explicit_directory).unwrap();
        let input = project.join("input.simf");
        let standard = explicit_directory.join("simfmt.toml");
        let dotfile = explicit_directory.join(".simfmt.toml");
        fs::write(&input, "").unwrap();
        fs::write(project.join("simfmt.toml"), "indent_width = 4\n").unwrap();
        fs::write(&standard, "indent_width = 2\n").unwrap();
        fs::write(&dotfile, "indent_width = 8\n").unwrap();

        let (config, path) = load_config(Some(&input), Some(&explicit_directory), None).unwrap();

        assert_eq!(path, Some(dotfile.canonicalize().unwrap()));
        assert_eq!(config.indent_width(), 8);
    }

    #[test]
    fn explicit_config_path_takes_precedence_over_discovery() {
        let directory = TestDirectory::new();
        let input = directory.path().join("input.simf");
        let explicit = directory.path().join("custom-config.toml");
        fs::write(&input, "").unwrap();
        fs::write(directory.path().join("simfmt.toml"), "indent_width = 2\n").unwrap();
        fs::write(&explicit, "indent_width = 8\n").unwrap();

        let (config, path) = load_config(Some(&input), Some(&explicit), None).unwrap();

        assert_eq!(path, Some(explicit.canonicalize().unwrap()));
        assert_eq!(config.indent_width(), 8);
    }

    #[test]
    fn unknown_options_are_ignored_while_known_options_load() {
        let directory = TestDirectory::new();
        let config_path = directory.path().join("custom.toml");
        fs::write(&config_path, "line_width = 144\nfuture_option = true\n").unwrap();

        let (config, path) = load_config(None, Some(&config_path), None).unwrap();

        assert_eq!(path, Some(config_path.canonicalize().unwrap()));
        assert_eq!(config.line_width(), 144);
    }

    #[test]
    fn missing_explicit_path_returns_contextual_error() {
        let directory = TestDirectory::new();
        let missing = directory.path().join("missing.toml");

        let error = load_config(None, Some(&missing), None).unwrap_err();

        assert!(error.to_string().contains("unable to find config file for path"));
        assert!(error.to_string().contains(&missing.display().to_string()));
    }

    #[test]
    fn explicit_directory_without_known_config_returns_contextual_error() {
        let directory = TestDirectory::new();
        let empty = directory.path().join("empty");
        fs::create_dir(&empty).unwrap();

        let error = load_config(None, Some(&empty), None).unwrap_err();

        assert!(error.to_string().contains("unable to find a simfmt config file in"));
        assert!(error.to_string().contains(&empty.display().to_string()));
    }

    #[test]
    fn malformed_toml_returns_parse_context() {
        let directory = TestDirectory::new();
        let config_path = directory.path().join("malformed.toml");
        fs::write(&config_path, "indent_width = [").unwrap();

        let error = load_config(None, Some(&config_path), None).unwrap_err();

        assert!(error.to_string().contains("invalid config"));
        assert!(error.to_string().contains(&config_path.display().to_string()));
    }

    #[test]
    fn invalid_config_value_returns_value_context() {
        let directory = TestDirectory::new();
        let config_path = directory.path().join("invalid-value.toml");
        fs::write(&config_path, "indent_width = \"wide\"\n").unwrap();

        let error = load_config(None, Some(&config_path), None).unwrap_err();

        assert!(error.to_string().contains("invalid config"));
        assert!(error.to_string().contains(&config_path.display().to_string()));
    }
}

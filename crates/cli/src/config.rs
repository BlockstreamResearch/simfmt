use anyhow::{Context, Error, anyhow, bail};
use prettysimf::config::{FmtConfig, PartialConfig};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Defines acceptable names for configuration discovery
const CONFIG_FILE_NAMES: [&str; 2] = [".simfmt.toml", "simfmt.toml"];

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
fn get_toml_path(dir: &Path) -> Result<Option<PathBuf>, std::io::Error> {
    for config_file_name in &CONFIG_FILE_NAMES {
        let config_file = dir.join(config_file_name);
        match fs::metadata(&config_file) {
            Ok(ref md) if md.is_file() => return Ok(Some(config_file.canonicalize()?)),
            Err(e) => {
                if !matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) {
                    return Err(e);
                }
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
pub fn resolve_project_file(dir: &Path) -> Result<Option<PathBuf>, Error> {
    let mut current = if dir.is_relative() {
        std::env::current_dir()?.join(dir)
    } else {
        dir.to_path_buf()
    };

    current = fs::canonicalize(current)?;

    loop {
        if let Some(path) = get_toml_path(&current)? {
            return Ok(Some(path));
        }

        if !current.pop() {
            break;
        }
    }

    if let Some(home) = home_dir() {
        if let Some(path) = get_toml_path(&home)? {
            return Ok(Some(path));
        }
    }

    if let Some(mut config) = config_dir() {
        config.push("simfmt");
        if let Some(path) = get_toml_path(&config)? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

fn resolve_explicit_config_path(path: &Path) -> Result<PathBuf, Error> {
    if !path.exists() {
        bail!(
            "Error: unable to find config file for the given path: `{}`",
            path.display()
        );
    }

    if path.is_dir() {
        get_toml_path(path)?
            .ok_or_else(|| anyhow!("Error: unable to find a simfmt config file in `{}`", path.display()))
    } else {
        path.canonicalize()
            .with_context(|| format!("Failed to canonicalize config path: {}", path.display()))
    }
}

fn read_config(path: &Path) -> Result<PartialConfig, Error> {
    let toml_str =
        fs::read_to_string(path).with_context(|| format!("Failed to read config file: {}", path.display()))?;

    let parsed: toml::Value = toml::from_str(&toml_str)
        .with_context(|| format!("Failed to parse TOML in config file: {}", path.display()))?;

    let Some(table) = parsed.as_table() else {
        bail!("Config file is not a TOML table: {}", path.display());
    };

    for key in table.keys() {
        if !FmtConfig::is_valid_key(key) {
            eprintln!("Warning: Unknown configuration option `{key}`");
        }
    }

    toml::from_str(&toml_str).with_context(|| format!("Failed to parse config values in: {}", path.display()))
}

/// Loads a config using an explicit config path when supplied, otherwise searching from the input path.
/// Command line overrides are applied last.
pub fn load_config(
    input_path: Option<&Path>,
    explicit_config_path: Option<&Path>,
    options_override: Option<PartialConfig>,
) -> Result<(FmtConfig, Option<PathBuf>), Error> {
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
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is before the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("simfmt-config-test-{}-{timestamp}", std::process::id()))
    }

    #[test]
    fn discovered_config_is_merged_before_cli_overrides() {
        let directory = test_directory();
        fs::create_dir_all(&directory).unwrap();
        let input = directory.join("input.simf");
        fs::write(&input, "").unwrap();
        fs::write(
            directory.join("simfmt.toml"),
            "indent_width = 2\nline_width = 80\nverbose = \"verbose\"\nemit_mode = \"stdout\"\nnewline_style = \"unix\"\n",
        )
        .unwrap();

        let cli = PartialConfig {
            line_width: Some(120),
            ..PartialConfig::default()
        };
        let (config, path) = load_config(Some(&input), None, Some(cli)).unwrap();

        assert_eq!(path, Some(directory.join("simfmt.toml").canonicalize().unwrap()));
        assert_eq!(config.indent_width(), 2);
        assert_eq!(config.line_width(), 120);
        assert_eq!(config.verbose(), prettysimf::utils::Verbosity::Verbose);
        assert_eq!(config.emit_mode(), prettysimf::utils::EmitMode::Stdout);
        assert_eq!(config.newline_style(), prettysimf::config::NewlineStyle::Unix);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explicit_config_path_takes_precedence_over_discovery() {
        let directory = test_directory();
        fs::create_dir_all(&directory).unwrap();
        let input = directory.join("input.simf");
        let explicit = directory.join("custom-config.toml");
        fs::write(&input, "").unwrap();
        fs::write(directory.join("simfmt.toml"), "indent_width = 2\n").unwrap();
        fs::write(&explicit, "indent_width = 8\n").unwrap();

        let (config, path) = load_config(Some(&input), Some(&explicit), None).unwrap();

        assert_eq!(path, Some(explicit.canonicalize().unwrap()));
        assert_eq!(config.indent_width(), 8);

        fs::remove_dir_all(directory).unwrap();
    }
}

use std::cell::Cell;
use std::io::Write;

use crate::emitter::EmitterConfig;
use crate::utils::{EmitMode, Verbosity};

/// Trait for values that can be used in formatter configuration.
pub(crate) trait ConfigType: Sized {
    fn doc_hint() -> String;
}

impl ConfigType for bool {
    fn doc_hint() -> String {
        String::from("<boolean>")
    }
}

impl ConfigType for usize {
    fn doc_hint() -> String {
        String::from("<unsigned integer>")
    }
}

impl ConfigType for Verbosity {
    fn doc_hint() -> String {
        String::from("[quiet|verbose]")
    }
}

impl ConfigType for EmitMode {
    fn doc_hint() -> String {
        String::from("[files|stdout|diff]")
    }
}

macro_rules! config_type {
    (
        $(#[$enum_meta:meta])*
        pub enum $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident),+ $(,)?
        }
    ) => {
        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        $(#[$enum_meta])*
        pub enum $name {
            $($(#[$variant_meta])* $variant),+
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        $name::$variant => write!(f, "{}", stringify!($variant)),
                    )+
                }
            }
        }

        impl ConfigType for $name {
            fn doc_hint() -> String {
                let mut variants = String::from("[");
                let mut first = true;
                $(
                    if !first {
                        variants.push('|');
                    }
                    first = false;
                    variants.push_str(stringify!($variant));
                )+
                variants.push(']');
                variants
            }
        }

        impl std::str::FromStr for $name {
            type Err = &'static str;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                $(
                    if stringify!($variant).eq_ignore_ascii_case(s) {
                        return Ok($name::$variant);
                    }
                )+
                Err(concat!("Bad variant, expected one of: ", $("`", stringify!($variant), "`", )+))
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                match self {
                    $(
                        $name::$variant => serializer.serialize_str(&stringify!($variant).to_ascii_lowercase()),
                    )+
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

config_type! {
    /// Controls whether formatter output uses terminal colors.
    pub enum Color {
        /// Requests colored output.
        Always,
        /// Disables colored output.
        Never,
        /// Uses colors when the output terminal supports them.
        Auto,
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Color {
    fn default() -> Self {
        Color::Auto
    }
}

impl Color {
    pub(crate) fn use_colored_tty(self) -> bool {
        match self {
            Color::Always | Color::Auto => true,
            Color::Never => false,
        }
    }
}

config_type! {
    /// Controls the line endings used in formatted output.
    pub enum NewlineStyle {
        /// Preserves the first line-ending style detected in the input.
        Auto,
        /// Uses Windows-style CRLF line endings.
        Windows,
        /// Uses Unix-style LF line endings.
        Unix,
        /// Uses the platform-native line-ending style.
        Native,
    }
}

#[allow(clippy::derivable_impls)]
impl Default for NewlineStyle {
    fn default() -> Self {
        NewlineStyle::Auto
    }
}

/// A slimmed-down version of rustfmt's `create_config!` macro.
///
/// `FmtConfig` stores resolved values and tracks both values supplied by the
/// user and values used during formatting. `PartialConfig` stores only the
/// options supplied by TOML or the command line.
macro_rules! create_config {
    ($($name:ident: $ty:ty = $default:expr, $description:expr;)+) => {
        /// Fully resolved formatter configuration.
        #[derive(Clone, Debug)]
        pub struct FmtConfig {
            // (was accessed, was explicitly set, value)
            $($name: (Cell<bool>, bool, $ty),)+
        }

        /// Optional configuration values used to override resolved settings.
        #[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq)]
        pub struct PartialConfig {
            $(
                #[doc = $description]
                pub $name: Option<$ty>,
            )+
        }

        pub struct ConfigSetter<'a>(&'a mut FmtConfig);

        impl<'a> ConfigSetter<'a> {
            $(
                pub fn $name(&mut self, value: $ty) {
                    self.0.$name.2 = value;
                }
            )+
        }

        pub struct ConfigWasSet<'a>(&'a FmtConfig);

        impl<'a> ConfigWasSet<'a> {
            $(
                pub fn $name(&self) -> bool {
                    self.0.$name.1
                }
            )+
        }

        impl PartialConfig {
            /// Replaces each option with the corresponding value present in `other`.
            pub fn merge_from(&mut self, other: &Self) {
                $(
                    if other.$name.is_some() {
                        self.$name = other.$name.clone();
                    }
                )+
            }

            /// Parses and sets one configuration option by name.
            ///
            /// # Errors
            ///
            /// Returns an error if `key` is unknown or `val` cannot be parsed as
            /// the option's value type.
            pub fn override_value(&mut self, key: &str, val: &str) -> Result<(), &'static str> {
                match key {
                    $(
                        stringify!($name) => {
                            self.$name = Some(val.parse().map_err(|_| "Failed to parse value")?);
                            Ok(())
                        }
                    )+
                    _ => Err("Unknown configuration option"),
                }
            }

            /// Serialize only project formatting options. Runtime controls are
            /// deliberately omitted, matching rustfmt's `PartialConfig::to_toml`.
            ///
            /// # Errors
            ///
            /// Returns an error if the project formatting options cannot be
            /// serialized as TOML.
            pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
                let mut config = self.clone();
                $(
                    if FmtConfig::is_hidden_option(stringify!($name)) {
                        config.$name = None;
                    }
                )+
                toml::to_string(&config)
            }
        }

        impl FmtConfig {
            $(
                #[doc = concat!("Returns the configured `", stringify!($name), "` value and records it as used.")]
                pub fn $name(&self) -> $ty {
                    self.$name.0.set(true);
                    self.$name.2.clone()
                }
            )+

            /// Returns a setter for updating resolved configuration values.
            pub fn set(&mut self) -> ConfigSetter<'_> {
                ConfigSetter(self)
            }

            /// Returns an accessor for checking which options were explicitly set.
            pub fn was_set(&self) -> ConfigWasSet<'_> {
                ConfigWasSet(self)
            }

            /// Applies every value present in `partial` to this configuration.
            pub fn apply_override(&mut self, partial: PartialConfig) {
                $(
                    if let Some(value) = partial.$name {
                        self.$name.1 = true;
                        self.$name.2 = value;
                    }
                )+
            }

            /// Returns whether `name` identifies a known configuration option.
            pub fn is_valid_key(name: &str) -> bool {
                matches!(name, $(stringify!($name))|+)
            }

            /// Returns whether `val` can be parsed for the option named by `key`.
            pub fn is_valid_key_val(key: &str, val: &str) -> bool {
                match key {
                    $(
                        stringify!($name) => val.parse::<$ty>().is_ok(),
                    )+
                    _ => false,
                }
            }

            /// Options used by formatting or output emission, preserving their
            /// partial form instead of converting every value to `Some`.
            pub fn used_options(&self) -> PartialConfig {
                PartialConfig {
                    $($name: self.$name.0.get().then(|| self.$name.2.clone()),)+
                }
            }

            /// Propagate access information from a cloned config. Formatting
            /// sessions use a clone while borrowing the session as an output
            /// handler, so the original config must retain those access marks.
            pub fn record_used_options(&self, used: &PartialConfig) {
                $(
                    if used.$name.is_some() {
                        self.$name.0.set(true);
                    }
                )+
            }

            /// All resolved options, used by `--print-config default/current`.
            pub fn all_options(&self) -> PartialConfig {
                PartialConfig {
                    $($name: Some(self.$name.2.clone()),)+
                }
            }

            /// Returns whether an option is an internal runtime control.
            pub fn is_hidden_option(name: &str) -> bool {
                const HIDE_OPTIONS: [&str; 4] = ["verbose" , "emit_mode", "color", "print_misformatted_file_names",];
                HIDE_OPTIONS.contains(&name)
            }

            /// Writes documentation for user-facing configuration options.
            pub fn print_docs(out: &mut dyn Write) {
                writeln!(out, "Configuration Options:").unwrap();
                $(
                    if !Self::is_hidden_option(stringify!($name)) {
                        writeln!(
                            out,
                            "  {} {} (Default: {:?})",
                            stringify!($name),
                            <$ty as ConfigType>::doc_hint(),
                            $default
                        )
                        .unwrap();
                        writeln!(out, "    {}", $description).unwrap();
                        writeln!(out).unwrap();
                    }
                )+
            }
        }

        impl Default for FmtConfig {
            fn default() -> Self {
                Self {
                    $($name: (Cell::new(false), false, $default),)+
                }
            }
        }
    };
}

create_config! {
    indent_width: usize = 4, "Number of spaces per tab";
    line_width: usize = 100, "Maximum width of each line";
    verbose: Verbosity = Verbosity::Quiet, "How much information to emit to the user";
    emit_mode: EmitMode = EmitMode::Files, "What data to emit and how";
    newline_style: NewlineStyle = NewlineStyle::Auto, "Unix or Windows line endings";
    color: Color = Color::Auto, "Use colored output (if supported)";
    print_misformatted_file_names: bool = false, "Print names of mismatched files";
}

impl std::fmt::Display for Verbosity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Verbosity::Quiet => write!(f, "Quiet"),
            Verbosity::Verbose => write!(f, "Verbose"),
        }
    }
}

impl std::fmt::Display for EmitMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitMode::Files => write!(f, "Files"),
            EmitMode::Stdout => write!(f, "Stdout"),
            EmitMode::Diff => write!(f, "Diff"),
        }
    }
}

impl FmtConfig {
    pub(crate) fn formatting_config(&self) -> InnerFmtConfig {
        InnerFmtConfig {
            indent_width: self.indent_width(),
            line_width: self.line_width(),
        }
    }

    pub(crate) fn get_emitter_conf(&self) -> EmitterConfig {
        EmitterConfig {
            print_misformatted_file_names: self.print_misformatted_file_names(),
            verbose: self.verbose(),
            color: self.color(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct InnerFmtConfig {
    pub indent_width: usize,
    pub line_width: usize,
}

impl Default for InnerFmtConfig {
    fn default() -> Self {
        Self {
            indent_width: 4,
            line_width: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_config_overrides_defaults_and_tracks_explicit_values() {
        let mut config = FmtConfig::default();
        config.apply_override(PartialConfig {
            indent_width: Some(2),
            verbose: Some(Verbosity::Verbose),
            ..PartialConfig::default()
        });

        assert!(config.was_set().indent_width());
        assert!(config.was_set().verbose());
        assert!(!config.was_set().line_width());
        assert_eq!(config.indent_width(), 2);
        assert_eq!(config.verbose(), Verbosity::Verbose);
    }

    #[test]
    fn used_options_only_contains_accessed_values() {
        let config = FmtConfig::default();
        let _ = config.indent_width();
        let _ = config.newline_style();

        assert_eq!(
            config.used_options(),
            PartialConfig {
                indent_width: Some(4),
                newline_style: Some(NewlineStyle::Auto),
                ..PartialConfig::default()
            }
        );
    }

    #[test]
    fn emitter_config_preserves_color() {
        let mut config = FmtConfig::default();
        config.set().color(Color::Always);

        assert_eq!(config.get_emitter_conf().color, Color::Always);
    }

    #[test]
    fn all_options_and_toml_omit_internal_controls() {
        let all_options = FmtConfig::default().all_options();
        assert_eq!(all_options.emit_mode, Some(EmitMode::Files));

        let toml = all_options.to_toml().unwrap();
        assert!(toml.contains("indent_width = 4"));
        assert!(toml.contains("newline_style = \"auto\""));
        assert!(!toml.contains("emit_mode"));
        assert!(!toml.contains("print_misformatted_file_names"));
        assert!(!toml.contains("color"));
    }
}

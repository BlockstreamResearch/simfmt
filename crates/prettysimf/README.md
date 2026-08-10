# prettysimf

`prettysimf` formats complete SimplicityHL source files. It provides a small
in-memory API and a reusable driver API for command-line tools and editor
integrations.

## Format in-memory source

```rust
use prettysimf::{FormatOptions, pretty_simf_please};

let source = "fn main(){assert!(jet::eq_1(param::FLAG,witness::BIT));}";
let formatted = pretty_simf_please(source.to_owned(), FormatOptions::default())?;
# Ok::<(), prettysimf::PrettySimfError>(())
```

`FormatOptions` contains the stable layout choices intended for normal library
users: indentation width, line width, and newline style.

## Driver API

The `prettysimf::driver` module exposes the shared configuration types, runtime
output options, and stateful formatting session. Project config discovery and
TOML loading remain private to the `simfmt` CLI.

```rust
use prettysimf::driver::{
    EmitMode, FmtConfig, FormatInput, FormatterSession, PartialConfig,
};

let mut config = FmtConfig::default();
config.apply_override(PartialConfig {
    emit_mode: Some(EmitMode::Stdout),
    ..PartialConfig::default()
});

let mut output = Vec::new();
let mut session = FormatterSession::new(config, Some(&mut output));
session.format_and_emit_report(FormatInput::Text("fn main() {}".to_owned()));

if !session.has_no_errors() {
    // Formatting failed or produced a check/diff error.
}
```

The session preserves the formatter's detailed error state internally while
exposing one stable success check to CLI and integration callers.

## License

MIT

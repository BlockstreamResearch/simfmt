# prettysimf

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Tests](https://github.com/BlockstreamResearch/simfmt/actions/workflows/crates.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/crates.yml)
[![Integration](https://github.com/BlockstreamResearch/simfmt/actions/workflows/fixtures.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/fixtures.yml)
[![Community](https://img.shields.io/endpoint?color=neon&logo=telegram&label=Chat&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fsimplicity_community)](https://t.me/simplicity_community)

`prettysimf` — in-memory formatting library for [SimplicityHL][1] code.

Formats complete SimplicityHL source files. It provides a small
in-memory API and a reusable driver API for command-line tools and editor
integrations.

## Format in-memory source

```rust
use prettysimf::{FormatOptions, NewlineStyle, pretty_simf_please};

# fn main() {
let source = "fn main(){assert!(jet::eq_1(param::FLAG,witness::BIT));}";
let options = FormatOptions {
    indent_width: 2,
    line_width: 80,
    newline_style: NewlineStyle::Unix,
};
let formatted = pretty_simf_please(source, options)?;
# Ok::<(), prettysimf::PrettySimfError>(())
# }
```

`FormatOptions` contains the stable layout choices intended for normal library
users:
* indentation width;
* line width;
* newline style.

## Errors

`pretty_simf_please` returns one of three `PrettySimfError` variants:

* `Operational` means the formatter could not complete an operational step.
* `FormatError` means the input could not be parsed or formatted safely; its
  message contains the formatter diagnostics.
* `StringConversion` means the formatted output unexpectedly contained invalid
  UTF-8.

The in-memory API has the same [formatter limitations][2] as the `simfmt` CLI.

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


## Contribution

See the repository's [contribution guide](../../Contributing.md) and
[code of conduct](../../CODE_OF_CONDUCT.md) before contributing.

## License

`prettysimf` is distributed under the [MIT License](../../LICENSE).

[1]: https://simplicity-lang.org/
[2]: https://github.com/BlockstreamResearch/simfmt#limitations

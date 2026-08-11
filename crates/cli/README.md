# simfmt

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Tests](https://github.com/BlockstreamResearch/simfmt/actions/workflows/crates.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/crates.yml)
[![Integration](https://github.com/BlockstreamResearch/simfmt/actions/workflows/fixtures.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/fixtures.yml)
[![Community](https://img.shields.io/endpoint?color=neon&logo=telegram&label=Chat&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fsimplicity_community)](https://t.me/simplicity_community)

SimplicityHL formatter tool

## Usage

```console
$ simfmt --help
Format SimplicityHL code

usage: simfmt [options] <file>...

Options:
        --emit [files|stdout]
                        What data to emit and how
        --config-path [Path for the configuration file]
                        Recursively searches the given path for the
                        simfmt.toml config file. If not found reverts to the
                        input file path
        --color [always|never|auto]
                        Use colored output (if supported)
        --print-config [default|current|minimal] PATH
                        Dumps a default, current, or minimal config to PATH. A
                        minimal config is the subset of the current config
                        file used for formatting the current program.
                        `current` writes to stdout current config as if
                        formatting the file at PATH.
    -l, --files-with-diff 
                        Prints the names of mismatched files that were
                        formatted. Prints the names of files that would be
                        formatted when used with `--check` mode.
        --check         Run in check mode without modifying files
        --config [key1=val1,key2=val2...]
                        Set options from command line. These settings take
                        priority over .simfmt.toml
    -v, --verbose       Print verbose output
    -q, --quiet         Print less output
    -V, --version       Show version information
    -h, --help [=TOPIC] Show this message or help about a specific topic:
                        `config`

```

`simfmt` formats complete SimplicityHL source files according to the project's
style rules. It can update files in place, write formatted source to standard
output, or verify formatting in CI.

## Installation

### From source

```sh
cargo +stable install simfmt --locked
```

Currently, installing `simfmt` requires rustc 1.91+.

### Locally
Install the formatter with Cargo:

```sh
cargo +stable install --path ./crates/cli/
```

## Usage

Format one or more files in place:

```sh
simfmt contract.simf module.simf
```

Read source from standard input and write the formatted result to standard
output:

```sh
echo 'fn     main() {}' | simfmt
```

Write formatted file input to standard output instead of modifying the file:

```sh
simfmt --emit stdout contract.simf
```

Run `simfmt --help` for the complete command-line reference and
`simfmt --help=config` for available configuration options.

## Checking formatting

Use `--check` to verify formatting without changing files:

```sh
simfmt --check contract.simf module.simf
```

### CI usage

1) In check mode, `simfmt` exits with status `0` when every input is formatted and
status `1` when it finds a difference or formatting error. Any generated diffs
are printed. This makes the command suitable for CI:

```sh
cargo install simfmt
simfmt --check src/main.simf
```

Use `--files-with-diff` with check mode when only the names of mismatched files
are needed.

2) For advanced CI usage you can use our own [.github/scripts/simfmt-check][2] bash script which checks all files
in directory for following the formatting rules.

## Configuration

Create `simfmt.toml` or `.simfmt.toml` in the project directory or one of its
parents. The closest discovered configuration is loaded, and values supplied
through `--config key=value` take precedence.

Generate a configuration containing all default formatting options:

```sh
simfmt --print-config default simfmt.toml
```

Configuration precedence is:

```text
defaults < config file < --config key=value
```

Other supported output modes are:

| Mode | Behavior |
| --- | --- |
| `files` | Overwrite input files when their formatting changes. |
| `stdout` | Write formatted source to standard output. |

Select a mode with `--emit`, for example:

```sh
simfmt --emit files contract.simf
```

## Editor integration

[SimplicityHL][1] formatting is available through the
[Visual Studio Code extension](https://marketplace.visualstudio.com/items?itemName=Blockstream.simplicityhl).

## Limitations

`simfmt` formats complete programs and requires input that can be parsed. Some
valid source constructs can still be unsupported. In particular:

- `use` declarations are not currently formatted.
- Complex comment placement may cause formatting to fail safely without
  modifying the input.
- [SimplicityHL][1] code blocks embedded in comments are not formatted.
- Formatting stability applies to complete programs, not arbitrary fragments.
- Non-ASCII source has less test coverage. (we believe Simfmt mostly works here, but do not have the test coverage
  or experience to be 100% sure).

## Contribution

See the repository's [contribution guide](../../Contributing.md) and
[code of conduct](../../CODE_OF_CONDUCT.md) before contributing.

## License

`simfmt` is distributed under the [MIT License](../../LICENSE).

[1]: https://simplicity-lang.org/
[2]: [https://simplicity-lang.org/](https://github.com/BlockstreamResearch/simfmt/blob/master/.github/scripts/simfmt-check)
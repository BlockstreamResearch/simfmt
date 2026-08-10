# simfmt CLI

`simfmt` formats complete SimplicityHL source files according to the project's
style rules. It can update files in place, write formatted source to standard
output, or verify formatting in CI.

The repository also contains `prettysimf`, the library used by this command-line
tool. This README covers the CLI only.

## Installation

Install the formatter with Cargo:

```sh
cargo install cargo-simfmt
```

The installed executable is `simfmt`.

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

In check mode, `simfmt` exits with status `0` when every input is formatted and
status `1` when it finds a difference or formatting error. Any generated diffs
are printed. This makes the command suitable for CI:

```sh
cargo install cargo-simfmt
simfmt --check src/main.simf
```

Use `--files-with-diff` with check mode when only the names of mismatched files
are needed.

## Configuration

Create `simfmt.toml` or `.simfmt.toml` in the project directory or one of its
parents. The closest discovered configuration is loaded, and values supplied
through `--config key=value` take precedence.

Generate a configuration containing all default formatting options:

```sh
simfmt --print-config default simfmt.toml
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

SimplicityHL formatting is available through the
[Visual Studio Code extension](https://marketplace.visualstudio.com/items?itemName=Blockstream.simplicityhl).

## Limitations

`simfmt` formats complete programs and requires input that can be parsed. Some
valid source constructs can still be unsupported. In particular:

- `use` declarations are not currently formatted.
- Complex comment placement may cause formatting to fail safely without
  modifying the input.
- SimplicityHL code blocks embedded in comments are not formatted.
- Formatting stability applies to complete programs, not arbitrary fragments.
- Non-ASCII source has less test coverage.

## Building and testing

From the repository root:

```sh
cargo build
cargo test
cargo run --bin simfmt -- contract.simf
```

See the repository's [contribution guide](../../Contributing.md) and
[code of conduct](../../CODE_OF_CONDUCT.md) before contributing.

## License

`simfmt` is distributed under the [MIT License](../../LICENSE).

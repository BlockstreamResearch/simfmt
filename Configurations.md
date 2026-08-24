# Configuring simfmt

Simfmt reads formatting options from `simfmt.toml` or `.simfmt.toml`. Put one
in the project directory or any parent directory. If none is found, simfmt also
checks the home directory and the `simfmt` directory under the user
configuration directory (for example, `~/.config/simfmt/`).

A project configuration should contain only formatting options:

```toml
indent_width = 4
line_width = 100
newline_style = "auto"
```

When formatting a file, values are selected in this order:

```text
defaults < simfmt.toml < --config key=value
```

## `indent_width`

Number of spaces used for one indentation level.

- Default: `4`
- Values: non-negative integer

## `line_width`

Maximum width used when laying out formatted code.

- Default: `100`
- Values: non-negative integer

## `newline_style`

Controls the line endings in the formatted result.

- Default: `"auto"`
- Values: `"auto"`, `"native"`, `"unix"`, `"windows"`

`auto` preserves the first line-ending style detected in each input file. If a
file has no line endings, it falls back to `native`. `native` uses `CRLF` on
Windows and `LF` on other platforms. `unix` always uses `LF`; `windows` always
uses `CRLF`.

## Internal CLI controls

The following are execution or output controls, not project formatting
settings. Use their command-line flags rather than putting them in
`simfmt.toml`:

- `emit_mode` — use `--emit files|stdout` or `--check`.
- `print_misformatted_file_names` — use `--files-with-diff` (`-l`).
- `color` — use `--color always|never|auto`.

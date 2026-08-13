# simfmt

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Tests](https://github.com/BlockstreamResearch/simfmt/actions/workflows/crates.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/crates.yml)
[![Integration](https://github.com/BlockstreamResearch/simfmt/actions/workflows/fixtures.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/fixtures.yml)
[![Community](https://img.shields.io/endpoint?color=neon&logo=telegram&label=Chat&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fsimplicity_community)](https://t.me/simplicity_community)


This repository consists form two crates, which can be used separately.

* [`simfmt`](./crates/cli/README.md)
  A tool for formatting [SimplicityHL][1] code according to style guidelines.
* [`prettysimf`](./crates/prettysimf/README.md)
  A library for formatting [SimplicityHL][1] code inplace in code. To emit already pretty formatted files.

If you'd like to help out, see [Contributing.md](Contributing.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).

## Using `prettysimf` as a library

Use `prettysimf` when formatting SimplicityHL source held in memory, such as in
an editor integration, language service, or another Rust application.

```rust
use prettysimf::{FormatOptions, pretty_simf_please};

fn main() {
    let source = "fn main(){assert!(true);}";
    let formatted =
        pretty_simf_please(source.to_owned(), FormatOptions::default())?;
}
```

## Using `simfmt` as a formatter

`simfmt` is a formatting tool performed in a unix style, so you can pipe data in or out to retrieve whatever output you want.

### Limitations

`simfmt` tries to work on as much [SimplicityHL][1] code as possible. The code doesn't even need to compile, but
need to be valid from AST point.
In general, we are looking to limit areas of instability; the formatting of most
code should not change as `simfmt` improves. However, there are some things that `simfmt` can't do or can't do well. We would like to reduce the list of limitations over time.

The following list enumerates areas where `simfmt` does not work or where the stability guarantees do not apply:

* a program where any part of the program does not parse.
* Uses declarations (current status: we don't formate them).
* Comments, including any AST node with a comment 'inside' (`simfmt` currently tries to format code with comments, but in
  tricky moments it can fail. We recommend using comments in predictable and simple use-cases. Don't worry, your code
  won't be chewed up by the formatter; it will just fail and won't touch your code in tricky places. We hope to fix this over time.)
* [SimplicityHL][1] code in code blocks in comments.
* Any fragment of a program (i.e., stability guarantees only apply to whole programs, even where fragments of a program
  can be formatted today).
* Code containing non-ascii unicode characters (we believe `simfmt` mostly works here, but do not have the test coverage
  or experience to be 100% sure).
* Bugs in `simfmt` (like any software, it has bugs, we do not consider bug fixes to break our stability guarantees).


### Installing the CLI

Install `simfmt` with cargo:

```sh
$ cargo install simfmt
```

### Running

To format individual files or arbitrary codes from stdin, `simfmt` binary should be used. 

This example will format `utils.simf` and `main.simf` inplace:

```sh
$ simfmt utils.simf main.simf
```

In this case `simfmt` will read a code from stdin and write formatting to stdout:
```sh
$ echo "fn     main() {}" | simfmt
fn main() {}
```

For more information, including arguments and emit options, see `simfmt --help`.

### Verifying code is formatted

* When running with `--check`, `simfmt` will exit with `0` if `simfmt` would not make any formatting changes to the input, and
`1` if `simfmt` would make changes.
* In other modes, `simfmt` will exit with `1` if there was some error during formatting
(for example a parsing or internal error) and `0` if formatting completed without error (whether or not changes were
made).

### Checking style in CI

To reinforce your checks in your repository via `simfmt` you can use these scripts, which are used in
our [CI configuration](.github/workflows/fixtures.yml) as an example.
In general, it uses `--check` instructs `simfmt` to exit with an error code if the input is not formatted correctly.

### Running `simfmt` from your editor

* [Visual Studio Code](https://marketplace.visualstudio.com/items?itemName=Blockstream.simplicityhl)

### Configuring `simfmt`

`simfmt` is designed to be very configurable. You can create a TOML file called
`simfmt.toml` or `.simfmt.toml`, place it in the project or any other parent directory and it will apply the options in
that file. See `simfmt --help=config` for the options which are available, or if you prefer to see visual style
previews.

### Tips

* When you run `simfmt`, place a file named `simfmt.toml` or `.simfmt.toml` in target file directory or its parents to
  override the default settings of simfmt. You can generate a file containing the default configuration and customize it.
```sh 
#$ simfmt --print-config [default|current|minimal] PATH
$ simfmt --print-config default simfmt.toml
```
* You can change the way simfmt emits the changes with the `--emit` flag:

```sh
$ simfmt <files> --emit [files|stdout]
 ```

## Appendix
To Read more information about `simfmt` go [here](./crates/cli/README.md) or about `prettysimf` go [here](./crates/prettysimf/README.md).

## License

Simfmt is distributed under the terms of both the MIT license.

See [MIT license](LICENSE) for details.


[1]: https://simplicity-lang.org/
# simfmt

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Tests](https://github.com/BlockstreamResearch/simfmt/actions/workflows/crates.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/crates.yml)
[![Integration](https://github.com/BlockstreamResearch/simfmt/actions/workflows/fixtures.yml/badge.svg?branch=master)](https://github.com/BlockstreamResearch/simfmt/workflows/fixtures.yml)
[![Community](https://img.shields.io/endpoint?color=neon&logo=telegram&label=Chat&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Fsimplicity_community)](https://t.me/simplicity_community)


This repository consists form two crates, which can be used separately.

* `simfmt`
  A tool for formatting [SimplicityHL][1] code according to style guidelines.
* `prettysimf`
  A library for formatting [SimplicityHL][1] code inplace in code. To emit already pretty formatted files.

If you'd like to help out (and you should, it's a fun project!), see
[Contributing.md](Contributing.md) and our [Code of Conduct](CODE_OF_CONDUCT.md).

## Using prettysimf as a library

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

## Installing the CLI

Install `simfmt` with cargo:

`cargo install simfmt`

## Limitations

Simfmt tries to work on as much [SimplicityHL][1] code as possible. Sometimes, the code doesn't even need to compile, but
need to be valid from tokens point! In general, we are looking to limit areas of instability; the formatting of most
code should not change as Simfmt improves. However, there are some things that Simfmt can't do or can't do well. We
would like to reduce the list of limitations over time.

The following list enumerates areas where Simfmt does not work or where the stability guarantees do not apply:

* a program where any part of the program does not parse.
* Uses declarations (current status: we don't formate them).
* Comments, including any AST node with a comment 'inside' (Simfmt currently tries to format code with comments, but in
  tricky moments it can fail. We recommend using comments in predictable and simple use-cases. Don't worry, your code
  won't be chewed up by the formatter; it will just fail and won't touch your code in tricky places. We hope to fix this
  over time.)
* [SimplicityHL][1] code in code blocks in comments.
* Any fragment of a program (i.e., stability guarantees only apply to whole programs, even where fragments of a program
  can be formatted today).
* Code containing non-ascii unicode characters (we believe Simfmt mostly works here, but do not have the test coverage
  or experience to be 100% sure).
* Bugs in Simfmt (like any software, Simfmt has bugs, we do not consider bug fixes to break our stability guarantees).

## Running

You can run Simfmt by just typing `simfmt filename` if you used `cargo
install`. This runs simfmt on the given file, if the file includes out of line modules, then we reformat those too.
Simfmt can also read data from stdin.

You can run `simfmt --help` for information about available arguments. The easiest way to run `simfmt` against a project
is with `prettysimf`.

### Running `simfmt` directly

To format individual files or arbitrary codes from stdin, the `simfmt` binary should be used. Some examples follow:

- `simfmt utils.simf main.simf` will format "utils.simf" and "main.simf" in place
- `simfmt` will read a code from stdin and write formatting to stdout
    - `echo "fn     main() {}" | simfmt` would emit `"fn main() {}"`.

For more information, including arguments and emit options, see `simfmt --help`.

### Verifying code is formatted

When running with `--check`, Simfmt will exit with `0` if Simfmt would not make any formatting changes to the input, and
`1` if Simfmt would make changes. In other modes, Simfmt will exit with `1` if there was some error during formatting
(for example a parsing or internal error) and `0` if formatting completed without error (whether or not changes were
made).

## Running Simfmt from your editor

* [Visual Studio Code](https://marketplace.visualstudio.com/items?itemName=Blockstream.simplicityhl)

## Checking style on a CI server

To keep your code base consistently formatted, it can be helpful to fail the CI build when a pull request contains
unformatted code. Using `--check` instructs simfmt to exit with an error code if the input is not formatted correctly.
It will also print any found differences.

To reinforce your checks in your repository via simfmt you can use these scripts, which are used in
our [CI configuration](.github/workflows/fixtures.yml) as an example.

## How to build and test

`cargo build` to build.

`cargo test` to run all tests.

To run simfmt after this, use `cargo run --bin simfmt -- filename`. See the notes above on running simfmt.

## Configuring Simfmt

Simfmt is designed to be very configurable. You can create a TOML file called
`simfmt.toml` or `.simfmt.toml`, place it in the project or any other parent directory and it will apply the options in
that file. See `simfmt --help=config` for the options which are available, or if you prefer to see visual style
previews.

## Tips

* When you run simfmt, place a file named `simfmt.toml` or `.simfmt.toml` in target file directory or its parents to
  override the default settings of simfmt. You can generate a file containing the default configuration with
  `simfmt --print-config default simfmt.toml` and customize as needed.
* After successful compilation, a `simfmt` executable can be found in the target directory.

* You can change the way simfmt emits the changes with the --emit flag:

  Example:

  ```sh
  simfmt <files> --emit files
  ```

  Options:

  | Flag |Description|
        |:---:|:---:|
  | files | overwrites output to files |
  | stdout | writes output to stdout |

## Appendix
To Read more information about `simfmt` go [here](./crates/cli/README.md) or about `prettysimf` go [here](./crates/prettysimf/README.md)

## License

Simfmt is distributed under the terms of both the MIT license.

See [MIT license](LICENSE) for details.


[1]: https://simplicity-lang.org/
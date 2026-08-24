# Design of simfmt

`simfmt` formats SimplicityHL source code. The command line binary is named
`simfmt`; the formatting library crate is named `prettysimf`.

The project borrows some formatter vocabulary from rustfmt, but its actual
implementation is specific to SimplicityHL. It uses the SimplicityHL parser and
formatter token stream, builds a pretty-printing document, renders that document
with a width budget, and then emits the result to files, stdout, or a diff.

## Goals

The main goal is a dependable formatter for complete SimplicityHL files:

* parse SimplicityHL source with the same grammar used by the language project
* format valid source into a stable, readable style
* preserve comments and source trivia whenever the parser exposes enough span
  information to attach them safely
* preserve source details that are not represented in the AST, such as selected
  punctuation spans and type-level decimal literal spelling
* provide CLI workflows for in-place formatting, stdout formatting, CI checks,
  and config inspection
* provide a small in-memory library API for tools that want formatted text
  without touching the filesystem
* support editor integrations that format complete, valid source held in memory

Non-goals for the current design:

* formatting arbitrary snippets that do not parse as a complete program
* formatting invalid SimplicityHL source
* refactoring, renaming, or reordering user code
* guessing where unsupported comments belong when surrounding syntax lacks
  usable spans

## Design Principles

### Preserve semantics

Formatting must not change the meaning of a program. The formatter may normalize
layout, whitespace, line breaks, separators, and other presentation details, but
it should not rename values, reorder declarations, rewrite expressions into a
different evaluation shape, or make edits that require semantic knowledge beyond
the parsed program.

### Prefer correctness over coverage

If the formatter cannot safely attach a comment or construct a document for a
valid program, it should report an error and leave file output unchanged rather
than silently dropping source text. This is especially important because
comments are carried outside the semantic AST in the formatter token stream.

### Use the parser for structure and tokens for lossless details

The AST gives the formatter reliable program structure. The formatter token
stream gives it source-backed details that the AST does not fully represent:
comments, whitespace trivia, semicolon positions, delimiter positions, comma
positions, and decimal literal spelling. `simfmt` is therefore a hybrid
formatter, not a pure AST formatter and not a token-only formatter.

### Keep formatting deterministic and idempotent

For any supported input, formatting the result again should produce the same
text. Unit tests and fixture tests should verify both the desired output and
idempotence as formatting support grows.

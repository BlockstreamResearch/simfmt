use crate::config::InnerFmtConfig;
use crate::error::ErrorKind;
use crate::simplicity_fmt::core::Context;
use crate::simplicity_fmt::doc::Doc;

use simplicityhl::parse::ParsedSource;

pub(crate) fn format_program(
    parsed: &ParsedSource<'_>,
    source: &str,
    config: &InnerFmtConfig,
) -> Result<String, ErrorKind> {
    let mut context = Context::new(config, source, parsed.tokens(), parsed.prefix().end);

    let doc = parsed
        .program()
        .to_doc(&mut context)
        .ok_or(ErrorKind::FailedToBuildDocument)?;

    if let Some(trivia) = context.trivia.remaining_comments().next() {
        let line = source[..trivia.span.start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1;
        return Err(ErrorKind::LostComment {
            line,
            start: trivia.span.start,
            end: trivia.span.end,
        });
    }

    let mut w = Vec::new();
    doc.render(config.line_width, &mut w)
        .map_err(|_| ErrorKind::FailedToRenderDocument)?;

    let formatted = String::from_utf8(w).map_err(ErrorKind::InvalidFormattedOutput)?;
    Ok(remove_whitespace_from_blank_lines(&formatted))
}

fn remove_whitespace_from_blank_lines(source: &str) -> String {
    let mut result = String::with_capacity(source.len());

    for line in source.split_inclusive('\n') {
        let (content, newline) = line.strip_suffix('\n').map_or((line, ""), |content| (content, "\n"));
        let (content, carriage_return) = content
            .strip_suffix('\r')
            .map_or((content, ""), |content| (content, "\r"));

        if content.chars().all(|character| matches!(character, ' ' | '\t')) {
            result.push_str(carriage_return);
            result.push_str(newline);
        } else {
            result.push_str(line);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use crate::error::ErrorKind;

    mod utils {
        use crate::config::InnerFmtConfig;
        use crate::error::ErrorKind;
        use crate::simplicity_fmt::fmt::format_program;
        use simplicityhl::UnstableFeatures;
        use simplicityhl::error::DiagnosticManager;
        use simplicityhl::parse::{ParsedSource, Program};

        const TEST_FILE_ID: usize = 0;
        const IDENT: &str = "    ";

        fn wrap_code_into_default_fn(code: &str) -> String {
            let indented_code = code
                .lines()
                .map(|line| format!("{IDENT}{line}"))
                .collect::<Vec<_>>()
                .join("\n");

            format!("fn main() {{\n{indented_code}\n}}")
        }

        fn assert_formatting_with_wrapping(source: &str, expected: &str, features: &UnstableFeatures) {
            let wrapped_source = wrap_code_into_default_fn(source);
            let wrapped_expected = wrap_code_into_default_fn(expected);

            assert_formatting(&wrapped_source, &wrapped_expected, features);
        }

        /// Wraps `source` and `expected` variables with an additional `main` function to make formatting step
        pub(super) fn assert_formatting_with_wrapping_in_all_modes(source: &str, expected: &str) {
            assert_formatting_with_wrapping(source, expected, &UnstableFeatures::none());
            assert_formatting_with_wrapping(source, expected, &UnstableFeatures::all());
        }

        fn assert_formatting(source: &str, expected: &str, features: &UnstableFeatures) {
            assert_formatting_with_config(source, expected, features, &InnerFmtConfig::default());
        }

        fn assert_formatting_with_config(
            source: &str,
            expected: &str,
            features: &UnstableFeatures,
            config: &InnerFmtConfig,
        ) {
            let formatted = format_source_with_config(source, features, config);
            assert_eq!(formatted, expected);
            assert_idempotent_with_config(expected, features, config);
        }

        pub(super) fn assert_formatting_stable(source: &str, expected: &str) {
            assert_formatting(source, expected, &UnstableFeatures::none())
        }

        pub(super) fn assert_formatting_unstable(source: &str, expected: &str) {
            assert_formatting(source, expected, &UnstableFeatures::all())
        }

        pub(super) fn assert_formatting_in_all_modes(source: &str, expected: &str) {
            assert_formatting_stable(source, expected);
            assert_formatting_unstable(source, expected);
        }

        pub(super) fn assert_formatting_in_all_modes_with_config(source: &str, expected: &str, config: InnerFmtConfig) {
            assert_formatting_with_config(source, expected, &UnstableFeatures::none(), &config);
            assert_formatting_with_config(source, expected, &UnstableFeatures::all(), &config);
        }

        fn parse_with<'src>(source: &'src str, features: &UnstableFeatures) -> ParsedSource<'src> {
            let mut diagnostics = DiagnosticManager::new();
            Program::parse_with_errors_for_fmt(TEST_FILE_ID, source, features, &mut diagnostics)
                .unwrap_or_else(|| panic!("source should parse for formatting:\n{diagnostics}"))
        }

        pub(super) fn format_source_stable(source: &str) -> String {
            format_source_with(source, &UnstableFeatures::none())
        }

        pub(super) fn format_error_stable(source: &str) -> ErrorKind {
            let features = UnstableFeatures::none();
            let parsed = parse_with(source, &features);
            format_program(&parsed, source, &InnerFmtConfig::default()).expect_err("source should fail to format")
        }

        pub(super) fn _format_source_unstable(source: &str) -> String {
            format_source_with(source, &UnstableFeatures::all())
        }

        fn format_source_with(source: &str, features: &UnstableFeatures) -> String {
            format_source_with_config(source, features, &InnerFmtConfig::default())
        }

        fn format_source_with_config(source: &str, features: &UnstableFeatures, config: &InnerFmtConfig) -> String {
            let parsed = parse_with(source, features);
            format_program(&parsed, source, config).expect("source formats")
        }

        pub(super) fn assert_idempotent_stable(formatted: &str) {
            assert_idempotent_with(formatted, &UnstableFeatures::none())
        }

        pub(super) fn assert_idempotent_unstable(formatted: &str) {
            assert_idempotent_with(formatted, &UnstableFeatures::all())
        }

        fn assert_idempotent_with(formatted: &str, features: &UnstableFeatures) {
            assert_idempotent_with_config(formatted, features, &InnerFmtConfig::default());
        }

        fn assert_idempotent_with_config(formatted: &str, features: &UnstableFeatures, config: &InnerFmtConfig) {
            assert_eq!(format_source_with_config(formatted, features, config), formatted);
        }

        pub(super) fn assert_comments_preserved<'a>(source: &str, comments: impl IntoIterator<Item = &'a str>) {
            let formatted = format_source_stable(source);

            for comment in comments {
                assert!(formatted.contains(comment), "missing {comment} in:\n{formatted}");
            }
            assert_idempotent_stable(&formatted);
            assert_idempotent_unstable(&formatted);
        }
    }

    #[test]
    fn preserves_comments_in_gaps_and_inside_items() {
        let source = "// leading\nfn first() { /* inside */ 1 }\n// between\nfn second() {} // trailing";

        utils::assert_comments_preserved(source, ["// leading", "/* inside */", "// between", "// trailing"]);
    }

    #[test]
    fn preserves_comments_in_match() {
        let source = r#"fn main() {
    match witness::PATH /* before match body */ {
        // before arm
        Left(x: u1) => /* after arrow */ {
            /* body */ x
        },
        // between arms
        Right(x: u2) => x,
    }
    // after match
}"#;

        utils::assert_comments_preserved(
            source,
            [
                "/* before match body */",
                "// before arm",
                "/* after arrow */",
                "/* body */",
                "// between arms",
                "// after match",
            ],
        );
    }

    #[test]
    fn comments_are_docs_while_surrounding_syntax_is_formatted() {
        let source = r#"// leading
fn   comments(a: u8, /* between parameters */ b: u8) /* before return arrow */ -> u8 {
let x: u8 = /* after equals */ add(a, /* between arguments */ b); // after statement
// before trailing expression
list![x, /* between elements */ b,]
}
// eof
"#;
        let formatted = utils::format_source_stable(source);

        for comment in [
            "// leading",
            "/* between parameters */",
            "/* before return arrow */",
            "/* after equals */",
            "/* between arguments */",
            "// after statement",
            "// before trailing expression",
            "/* between elements */",
            "// eof",
        ] {
            assert!(formatted.contains(comment), "missing {comment} in:\n{formatted}");
        }
        assert!(formatted.contains("fn comments("));
        assert!(formatted.contains("let x: u8 = /* after equals */ add("));
        utils::assert_idempotent_stable(&formatted);
        utils::assert_idempotent_unstable(&formatted);
    }

    #[test]
    fn preserves_multiline_block_comments_as_indented_doc_lines() {
        let source = "fn main() {\n    /* first line\n       second line */\n    ()\n}";
        let formatted = utils::format_source_stable(source);

        assert!(formatted.contains("    /* first line\n       second line */"));
        utils::assert_idempotent_stable(&formatted);
        utils::assert_idempotent_unstable(&formatted);
    }

    #[test]
    fn reports_comments_inside_spanless_parameter_syntax() {
        let source = "fn main(value /* no identifier/type boundary */: u8) {}";
        let start = source.find("/*").unwrap();
        let end = start + "/* no identifier/type boundary */".len();

        match utils::format_error_stable(source) {
            ErrorKind::LostComment {
                line,
                start: actual_start,
                end: actual_end,
            } => {
                assert_eq!(line, 1);
                assert_eq!(actual_start, start);
                assert_eq!(actual_end, end);
            }
            other => panic!("expected unsupported comment, got {other:?}"),
        }
    }

    #[test]
    fn preserves_comment_only_files() {
        let source = "/* outer /* nested */ outer */\r\n// eof\r\n";
        let expected = "/* outer /* nested */ outer */\n// eof\n";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn preserves_simc_prefix_and_following_comments() {
        let source = "// header\nsimc \"*\";\n// body\nfn main() {}";
        let formatted = utils::format_source_stable(source);

        assert!(formatted.starts_with("// header\nsimc \"*\";"));
        assert!(formatted.contains("// body"));
        utils::assert_idempotent_stable(&formatted);
        utils::assert_idempotent_unstable(&formatted);
    }

    #[test]
    fn removes_leading_newlines_without_a_preamble() {
        let source = "\n \n\t\r\nfn main() {}";
        let expected = "fn main() {}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn removes_leading_newlines_before_a_comment() {
        let source = "\n\n  \n// header\nfn main() {}";
        let expected = "// header\nfn main() {}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn trims_leading_newlines_and_shortens_the_simc_preamble_gap() {
        let source = "\n\n simc \"*\";\n\n\nfn main() {}";
        let expected = "simc \"*\";\n\nfn main() {}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn long_preserved_source_fragments_do_not_change_following_layout() {
        let aliases = r#"// Binary LMSR Pool Covenant (Unified Source)
// SimplicityHL contract for Liquid.
//
// This source defines both leaf entry functions:
// - `lmsr_primary_main(...)`: swap/admin path with full state transition checks.
// - `lmsr_secondary_main(...)`: NO/collateral co-membership path.
//
// `main` wrappers are appended per-leaf by Rust compilation code.

type PathPrimary = Either<(), ()>;
type ScanPayload = (u32, (u64, (u64, u8)));"#;
        utils::assert_formatting_in_all_modes(aliases, aliases);

        let commented_function = r#"fn preserved() -> bool {
    // This deliberately long preserved comment keeps the whole function on the source-aware formatting path.
    true
}
fn compact(bit: bool) -> bool {
    bit
}"#;
        utils::assert_formatting_in_all_modes(commented_function, commented_function);

        let prefixed = r#"// This deliberately long prefix must not consume the pretty-printer column budget for the following item.
simc "*";
type PathPrimary = Either<(), ()>;"#;
        utils::assert_formatting_in_all_modes(prefixed, prefixed);
    }

    #[test]
    fn formats_enum_declarations_constructions_and_matches() {
        let source = "enum Action{Stop,Refresh(u8,bool),}\n\
                      fn main(value:Action){\
                      let next:Action=Action::Refresh(1,true);\
                      match value{\
                      Action::Stop=>{{{{()}}}},\
                      Action::Refresh(number:u8,flag:bool)=>{number}\
                      }}";

        let expected = r"enum Action {
    Stop,
    Refresh(u8, bool),
}
fn main(value: Action) {
    let next: Action = Action::Refresh(1, true);
    match value {
        Action::Stop => (),
        Action::Refresh(number: u8, flag: bool) => number,
    }
}";

        utils::assert_formatting_unstable(source, expected);
    }

    #[test]
    fn formats_option_and_bool_matches() {
        let source = "fn main(value:Option<bool>,flag:bool){\
                      let next:Option<bool>=Some(true);\
                      match value{\
                      None=>{{{panic!()}}}\
                      Some(inner:bool)=>Some(inner),\
                      };\
                      match flag{\
                      false=>false,\
                      true=>{{{{true}}}}\
                      }}";

        let expected = r"fn main(value: Option<bool>, flag: bool) {
    let next: Option<bool> = Some(true);
    match value {
        Some(inner: bool) => Some(inner),
        None => panic!(),
    };
    match flag {
        true => true,
        false => false,
    }
}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn preserves_blank_lines_between_comment_free_items() {
        let source = "fn one() {}\n\n\nfn two() {}";
        let expected = "fn one() {}\n\nfn two() {}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn wraps_long_tuple_and_array_patterns_with_their_bindings() {
        let source = r#"
fn main(protocol_fee_vault_indexes: (u32, u32), protocol_fee_vault_array_indexes: [u32; 2]) {
    let (protocol_fee_vault_input_index, protocol_fee_vault_output_index): (u32, u32) = protocol_fee_vault_indexes;
    let [protocol_fee_vault_input_index, protocol_fee_vault_output_index]: [u32; 2] = protocol_fee_vault_array_indexes;
}
"#;

        let formatted = utils::format_source_stable(source);
        assert!(formatted.contains(
            "let (\n        protocol_fee_vault_input_index,\n        protocol_fee_vault_output_index\n    ): (u32, u32) = protocol_fee_vault_indexes;"
        ));
        assert!(formatted.contains(
            "let [\n        protocol_fee_vault_input_index,\n        protocol_fee_vault_output_index\n    ]: [u32; 2] = protocol_fee_vault_array_indexes;"
        ));
        utils::assert_idempotent_stable(&formatted);
        utils::assert_idempotent_unstable(&formatted);
    }

    #[test]
    fn preserves_blank_lines_between_block_statements() {
        let source = r#"
fn main() {
    let first: u32 = 1;
    let second: u32 = 2;

    let third: u32 = 3;
}
"#;

        let expected = r#"fn main() {
    let first: u32 = 1;
    let second: u32 = 2;

    let third: u32 = 3;
}
"#;

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn unwraps_single_expression_match_arms_inside_commented_functions() {
        let source = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => {
            false
        },
        true => {
            true
        },
    };
    match true {
        true => {
            true
        },
        false => {
            false
        }
    }
}
"#;
        let expected = r#"fn main() {
    // Keep this function source-aware.
    match true {
        true => true,
        false => false,
    };
    match true {
        true => true,
        false => false,
    }
}
"#;

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn keeps_fitting_function_calls_inline_after_unwrapping_nested_blocks() {
        let source = "match flag{
    true => {{{safe_subtract(out_no_amount, in_no_amount)}}},
    false => {{safe_subtract(in_no_amount, out_no_amount)}},
}";
        let expected = "match flag {
    true => safe_subtract(out_no_amount, in_no_amount),
    false => safe_subtract(in_no_amount, out_no_amount),
}";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn uses_blocks_when_match_arm_calls_need_multiple_lines() {
        let source = "fn main(flag: bool) {
    match flag {
        true => calculate(first_argument, second_argument),
        false => calculate(second_argument, first_argument),
    }
}";
        let expected = "fn main(flag: bool) {
    match flag {
        true => {
            calculate(
                first_argument,
                second_argument
            )
        },
        false => {
            calculate(
                second_argument,
                first_argument
            )
        },
    }
}";
        let config = crate::config::InnerFmtConfig {
            indent_width: 4,
            line_width: 44,
        };

        utils::assert_formatting_in_all_modes_with_config(source, expected, config);
    }

    #[test]
    fn unwraps_empty_tuples_and_nested_matches_inside_commented_functions() {
        let source = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => (),
        true => match false {
            true => true,
            false => false,
        },
    }
}
"#;
        let expected = r#"fn main() {
    // Keep this function source-aware.
    match true {
        true => match false {
            true => true,
            false => false,
        },
        false => (),
    }
}
"#;

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn removes_whitespace_from_blank_lines() {
        assert_eq!(
            super::remove_whitespace_from_blank_lines("one\n \t\n\t \r\ntwo\n   "),
            "one\n\n\r\ntwo\n"
        );
    }

    #[test]
    fn omit_trailing_comas_in_tuples() {
        let source = "let pair:(u8,bool,)=(1,true,);
let singleton:(u8,)=(1,);";
        let expected = "let pair: (u8, bool) = (1, true);
let singleton: (u8) = (1);";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn alias() {
        let source = "type Payload=Either<Option<[u8;10]>,List<(bool,u16),4>>;";
        let expected = "type Payload = Either<Option<[u8; 10]>, List<(bool, u16), 4>>;";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn indents_the_first_element_of_a_wrapped_tuple_type() {
        let source = "type Payload=(u32,(u64,u8));";
        let expected = "type Payload = (\n    u32,\n    (u64, u8)\n);";
        let config = crate::config::InnerFmtConfig {
            indent_width: 4,
            line_width: 24,
        };

        utils::assert_formatting_in_all_modes_with_config(source, expected, config);

        let expected = "type Payload = (u32, (u64, u8));";
        let config = crate::config::InnerFmtConfig {
            indent_width: 4,
            line_width: 100,
        };

        utils::assert_formatting_in_all_modes_with_config(source, expected, config);
    }

    #[test]
    fn wraps_function_parameters_before_a_compact_return_type() {
        let source =
            "fn get_asset_issuance_issuance_factory_indexes(start_input_index:u32,start_output_index:u32)->(u32,u32){}";
        let expected = "fn get_asset_issuance_issuance_factory_indexes(\n    start_input_index: u32,\n    start_output_index: u32\n) -> (u32, u32) {}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn true_false_reordering() {
        let source = "let x: bool = match true {
    false => false,
    true => true,
};
let x: bool = match true {
    true => true,
    false => false,
};";
        let expected = r#"let x: bool = match true {
    true => true,
    false => false,
};
let x: bool = match true {
    true => true,
    false => false,
};"#;

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn true_false_reordering_and_code_wrapping() {
        let source = "match true {
    false => {{false}},
    true => true,}";
        let expected = "match true {
    true => true,
    false => false,
}";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn some_reordering_and_code_wrapping() {
        let source = "match witness::PATH {
    None => None,
    Some(value: bool) => Some(value),
}";
        let expected = r#"match witness::PATH {
    Some(value: bool) => Some(value),
    None => None,
}"#;

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn either_reordering() {
        let source = "match witness::PATH {
    Right(value: bool) => Right(value),
    Left(value: bool) => Left(value),
}";
        let expected = "match witness::PATH {
    Left(value: bool) => Left(value),
    Right(value: bool) => Right(value),
}";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn nested_match_arm_bodies_do_not_add_extra_indent() {
        let source = "match witness::PATH {
    Right(value: bool) => match value {
        false => false,
        true => true,
    },
    Left(value: bool) => match value {
        false => false,
        true => true,
    },
}";
        let expected = "match witness::PATH {
    Left(value: bool) => match value {
        true => true,
        false => false,
    },
    Right(value: bool) => match value {
        true => true,
        false => false,
    },
}";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn enum_dont_preserve_order() {
        let source = "enum EnumValues {One,Two(u8, bool),Three(List<u16, 8>, bool, u8),Four((u8, u64)),}

fn random_fn(value: EnumValues) {
    let next: EnumValues = EnumValues::One;let num: u8 = match value {
EnumValues::One => 0,
EnumValues::Three(list: List<u16, 8>, bool_val: bool, num: u8) => num,
EnumValues::Two(num: u8, flag: bool) => num,
EnumValues::Four(tuple: (u8, u64)) => {
    let (num, x): (u8, u64) = tuple;
    num
},};
}";

        let expected = r#"enum EnumValues {
    One,
    Two(u8, bool),
    Three(List<u16, 8>, bool, u8),
    Four((u8, u64)),
}

fn random_fn(value: EnumValues) {
    let next: EnumValues = EnumValues::One;
    let num: u8 = match value {
        EnumValues::One => 0,
        EnumValues::Three(list: List<u16, 8>, bool_val: bool, num: u8) => num,
        EnumValues::Two(num: u8, flag: bool) => num,
        EnumValues::Four(tuple: (u8, u64)) => {
            let (num, x): (u8, u64) = tuple;
            num
        },
    };
}"#;
        //TODO: maybe remove trailing coma for multiline MatchArm?

        utils::assert_formatting_unstable(source, expected);
    }

    #[test]
    fn list_trailling_coma() {
        let source = r#"fn main() {
    let values:List<u8,4>=list![1,2,3,4,];
}"#;
        let expected = r#"fn main() {
    let values: List<u8, 4> = list![1, 2, 3, 4];
}"#;

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn line_nesting() {
        let source = "fn main() {
    let list_arg:List<Either<(u1,u2,u4,u8),(bool,Option<[u8;10]>,u16,u32,u64,u128,u256)>,256>=witness::DRAFT;
}";
        let expected = "fn main() {
    let list_arg: List<
        Either<(u1, u2, u4, u8), (bool, Option<[u8; 10]>, u16, u32, u64, u128, u256)>, 256
    > = witness::DRAFT;
}";

        utils::assert_formatting_in_all_modes(source, expected);
    }

    #[test]
    fn structs_trailing_coma() {
        let source = "enum Pair{Value(u8,bool,),}
fn main(){let pair:Pair=Pair::Value(1,true,);}";
        let expected = "enum Pair {
    Value(u8, bool),
}
fn main() {
    let pair: Pair = Pair::Value(1, true);
}";

        utils::assert_formatting_unstable(source, expected);
    }

    #[test]
    fn preserve_underscores_in_numbers() {
        let source = "let x     :u64 = 0x00_11_22_33_44_55_66_77;\n\
                            let y:u16 =   0b0000_1111_0000_1111;\n\
                            let z:    u32   = 123456789    ;";
        let expected = "let x: u64 = 0x00_11_22_33_44_55_66_77;\n\
                               let y: u16 = 0b0000_1111_0000_1111;\n\
                               let z: u32 = 123456789;";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn preserve_underscores_in_lists() {
        let source = "let bytes     :List<u8,1_024> = witness::BYTES;\n\
                            let chunks:List<[u8;1_024],2_048>=witness::CHUNKS;\n\
                            let nested: List<List<u16, 1_024_>, 4_096> = witness::NESTED;\n\
                            let values:List<u16,4>=list![1_000,2_000,3_000,4_000,];";
        let expected = "let bytes: List<u8, 1_024> = witness::BYTES;\n\
                               let chunks: List<[u8; 1_024], 2_048> = witness::CHUNKS;\n\
                               let nested: List<List<u16, 1_024_>, 4_096> = witness::NESTED;\n\
                               let values: List<u16, 4> = list![1_000, 2_000, 3_000, 4_000];";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }

    #[test]
    fn preserve_underscores_in_arrays() {
        let source = "let bytes     :[u8;1_024] = witness::BYTES;\n\
                            let nested:[[u16;1_024_];4_096]=witness::NESTED;\n\
                            let values:[u16;4]=[1_000,2_000,3_000,4_000,];";
        let expected = "let bytes: [u8; 1_024] = witness::BYTES;\n\
                               let nested: [[u16; 1_024_]; 4_096] = witness::NESTED;\n\
                               let values: [u16; 4] = [1_000, 2_000, 3_000, 4_000];";

        utils::assert_formatting_with_wrapping_in_all_modes(source, expected);
    }
}

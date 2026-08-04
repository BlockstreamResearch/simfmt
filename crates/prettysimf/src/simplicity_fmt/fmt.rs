use crate::config::InnerFmtConfig;
use crate::error::ErrorKind;
use crate::simplicity_fmt::core::Context;
use crate::simplicity_fmt::doc::Doc;
use simplicityhl::parse::ParsedSource;

pub fn format_program(parsed: &ParsedSource<'_>, source: &str, config: &InnerFmtConfig) -> Result<String, ErrorKind> {
    let mut context = Context::new(config, source, parsed.tokens(), parsed.prefix().end);

    let doc = parsed
        .program()
        .to_doc(&mut context)
        .ok_or(ErrorKind::FailedToBuildDocument)?;

    if let Some(trivia) = context.trivia.remaining_comments().next() {
        return Err(ErrorKind::LostComment(trivia.span.start..trivia.span.end));
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
    mod utils {
        use crate::config::InnerFmtConfig;
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
            let formatted = format_source_with(source, features);
            assert_eq!(formatted, expected);
            assert_idempotent_with(expected, features);
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

        fn parse_with<'src>(source: &'src str, features: &UnstableFeatures) -> ParsedSource<'src> {
            let mut diagnostics = DiagnosticManager::new();
            Program::parse_with_errors_for_fmt(TEST_FILE_ID, source, features, &mut diagnostics)
                .unwrap_or_else(|| panic!("source should parse for formatting:\n{diagnostics}"))
        }

        pub(super) fn format_source_stable(source: &str) -> String {
            format_source_with(source, &UnstableFeatures::none())
        }

        pub(super) fn format_source_unstable(source: &str) -> String {
            format_source_with(source, &UnstableFeatures::all())
        }

        fn format_source_with(source: &str, features: &UnstableFeatures) -> String {
            let parsed = parse_with(source, features);
            format_program(&parsed, source, &InnerFmtConfig::default()).expect("source formats")
        }

        pub(super) fn assert_idempotent_stable(formatted: &str) {
            assert_idempotent_with(formatted, &UnstableFeatures::none())
        }

        pub(super) fn assert_idempotent_unstable(formatted: &str) {
            assert_idempotent_with(formatted, &UnstableFeatures::all())
        }

        fn assert_idempotent_with(formatted: &str, features: &UnstableFeatures) {
            assert_eq!(format_source_with(formatted, features), formatted);
        }

        pub(super) fn assert_comments_preserved<'a>(source: &str, comments: impl IntoIterator<Item = &'a str>) {
            let formatted = format_source_stable(source);

            for comment in comments {
                assert!(formatted.contains(comment), "missing {comment}");
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
    match /* scrutinee */ witness::PATH {
        // before arm
        Left(x: /* binding type */ u1) => {
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
                "/* scrutinee */",
                "// before arm",
                "/* binding type */",
                "/* body */",
                "// between arms",
                "// after match",
            ],
        );
    }

    #[test]
    fn preserves_comment_only_files() {
        let source = "/* outer /* nested */ outer */\r\n// eof\r\n";
        assert_eq!(utils::format_source_stable(source), source);
        assert_eq!(utils::format_source_unstable(source), source);
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

        utils::assert_formatting_in_all_modes(source, source);
    }

    #[test]
    fn indents_inline_match_arms_inside_commented_functions() {
        let source = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => false,
        true => true,
    };
    match true {
        true => true,
        false => false,
    }
}
"#;
        let expected = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => false,
        true => true,
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

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
    use super::{InnerFmtConfig, format_program};
    use simplicityhl::UnstableFeatures;
    use simplicityhl::error::DiagnosticManager;
    use simplicityhl::parse::{ParsedSource, Program};

    const TEST_FILE_ID: usize = 0;

    fn parse_with<'src>(source: &'src str, features: &UnstableFeatures) -> ParsedSource<'src> {
        let mut diagnostics = DiagnosticManager::new();
        Program::parse_with_errors_for_fmt(TEST_FILE_ID, source, features, &mut diagnostics)
            .unwrap_or_else(|| panic!("source should parse for formatting:\n{diagnostics}"))
    }

    fn format_source(source: &str) -> String {
        format_source_with(source, &UnstableFeatures::none())
    }

    fn format_source_with(source: &str, features: &UnstableFeatures) -> String {
        let parsed = parse_with(source, features);
        format_program(&parsed, source, &InnerFmtConfig::default()).expect("source formats")
    }

    fn assert_idempotent(formatted: &str) {
        assert_eq!(format_source(formatted), formatted);
    }

    fn assert_idempotent_with(formatted: &str, features: &UnstableFeatures) {
        assert_eq!(format_source_with(formatted, features), formatted);
    }

    fn assert_comments_preserved<'a>(source: &str, comments: impl IntoIterator<Item = &'a str>) {
        let formatted = format_source(source);

        for comment in comments {
            assert!(formatted.contains(comment), "missing {comment}");
        }
        assert_idempotent(&formatted);
    }

    #[test]
    fn preserves_comments_in_gaps_and_inside_items() {
        let source = "// leading\nfn first() { /* inside */ 1 }\n// between\nfn second() {} // trailing";

        assert_comments_preserved(source, ["// leading", "/* inside */", "// between", "// trailing"]);
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

        assert_comments_preserved(
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
        assert_eq!(format_source(source), source);
    }

    #[test]
    fn preserves_simc_prefix_and_following_comments() {
        let source = "// header\nsimc \"*\";\n// body\nfn main() {}";
        let formatted = format_source(source);

        assert!(formatted.starts_with("// header\nsimc \"*\";"));
        assert!(formatted.contains("// body"));
        assert_idempotent(&formatted);
    }

    #[test]
    fn formats_enum_declarations_constructions_and_matches() {
        let source = "enum Action{Stop,Refresh(u8,bool),}\n\
                      fn main(value:Action){\
                      let next:Action=Action::Refresh(1,true);\
                      match value{\
                      Action::Stop=>(),\
                      Action::Refresh(number:u8,flag:bool)=>number,\
                      }}";
        let features = UnstableFeatures::all();
        let formatted = format_source_with(source, &features);

        assert_eq!(
            formatted,
            r#"enum Action {
    Stop,
    Refresh(u8, bool),
}
fn main(value: Action) {
    let next: Action = Action::Refresh(1, true);
    match value {
        Action::Stop => {},
        Action::Refresh(number: u8, flag: bool) => {
            number
        }
    }
}"#
        );
        assert_idempotent_with(&formatted, &features);
    }

    #[test]
    fn preserves_blank_lines_between_comment_free_items() {
        let source = "fn one() {}\n\n\nfn two() {}";
        let formatted = format_source(source);

        assert_eq!(formatted, "fn one() {}\n\nfn two() {}");
    }

    #[test]
    fn wraps_long_tuple_and_array_patterns_with_their_bindings() {
        let source = r#"
fn main(protocol_fee_vault_indexes: (u32, u32), protocol_fee_vault_array_indexes: [u32; 2]) {
    let (protocol_fee_vault_input_index, protocol_fee_vault_output_index): (u32, u32) = protocol_fee_vault_indexes;
    let [protocol_fee_vault_input_index, protocol_fee_vault_output_index]: [u32; 2] = protocol_fee_vault_array_indexes;
}
"#;

        let formatted = format_source(source);
        assert!(formatted.contains(
            "let (\n        protocol_fee_vault_input_index,\n        protocol_fee_vault_output_index\n    ): (u32, u32) = protocol_fee_vault_indexes;"
        ));
        assert!(formatted.contains(
            "let [\n        protocol_fee_vault_input_index,\n        protocol_fee_vault_output_index\n    ]: [u32; 2] = protocol_fee_vault_array_indexes;"
        ));
        assert_idempotent(&formatted);
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

        assert_eq!(format_source(source), source);
    }

    #[test]
    fn indents_inline_match_arms_inside_commented_functions() {
        let source = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => false,
        true => true,
    }
}
"#;
        let expected = r#"
fn main() {
    // Keep this function source-aware.
    match true {
        false => {
            false
        },
        true => {
            true
        },
    }
}
"#;

        let formatted = format_source(source);
        assert_eq!(formatted, expected);
        assert_idempotent(&formatted);
    }

    #[test]
    fn removes_whitespace_from_blank_lines() {
        assert_eq!(
            super::remove_whitespace_from_blank_lines("one\n \t\n\t \r\ntwo\n   "),
            "one\n\n\r\ntwo\n"
        );
    }
}

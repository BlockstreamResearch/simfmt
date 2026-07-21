use crate::config::InnerFmtConfig;
use crate::simplicity_fmt::core::Context;
use crate::simplicity_fmt::doc::Doc;
use simplicityhl::parse::ParsedSource;

pub fn format_program(parsed: &ParsedSource<'_>, source: &str, config: &InnerFmtConfig) -> Result<String, String> {
    let mut context = Context::new(config, source, parsed.tokens(), parsed.prefix().end);

    let doc = parsed
        .program()
        .to_doc(&mut context)
        .ok_or("Failed to produce doc for program")?;

    if let Some(trivia) = context.trivia.remaining_comments().next() {
        return Err(format!(
            "formatter did not attach comment at {}..{}",
            trivia.span.start, trivia.span.end
        ));
    }

    let mut w = Vec::new();
    doc.render(config.line_width, &mut w)
        .map_err(|e| format!("Failed to render doc: {}", e))?;

    let formatted = String::from_utf8(w).map_err(|e| format!("Failed to convert rendered doc to string: {}", e))?;
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
    use simplicityhl::parse::Program;
    use simplicityhl::source::SourceFile;
    use std::sync::Arc;

    fn format(source: &str) -> String {
        let source_file = SourceFile::anonymous(Arc::from(source));
        let parsed = Program::parse_for_formatting(0, &source_file, &UnstableFeatures::none())
            .expect("source parses for formatting");
        format_program(&parsed, source, &InnerFmtConfig::default()).expect("source formats")
    }

    //todo: remove or refactosr these complex tests
    #[test]
    fn preserves_comments_in_gaps_and_inside_items() {
        let line_comment = ["// line1", "// line2", "// line3", "// line4", "// line5"];
        let block_comment = [
            "/* block1 */",
            "/* block2 */",
            "/* block3 */",
            "/* block4 */",
            "/* block5 */",
            "/* block6 */",
        ];
        // let source = "// leading\nfn first() { /* inside */ 1 }\n// between\nfn second() {} // eof\n";
        let source = format!(
            "{}
fn              {}               first()                {} {{
 {}1{}
}}
 {}
 {}

fn                         {}                 second()                  {}
{{
                                                    {}
}}

{}",
            line_comment[0],
            block_comment[0],
            block_comment[1],
            block_comment[2],
            line_comment[1],
            line_comment[2],
            line_comment[3],
            block_comment[3],
            block_comment[4],
            block_comment[5],
            line_comment[4],
        );

        // let formatted = dbg!(format(dbg!(&source)));
        let formatted = format(&source);

        for comment in line_comment.iter().chain(block_comment.iter()) {
            assert!(formatted.contains(comment), "missing {comment}");
        }
        assert_eq!(format(&formatted), formatted);
        let source_file = SourceFile::anonymous(Arc::from(formatted.as_str()));
        Program::parse_for_formatting(0, &source_file, &UnstableFeatures::none()).expect("formatted source reparses");
    }

    //todo: remove or refactosr these complex tests
    #[test]
    fn preserves_comments_in_match() {
        let line_comment = [
            "// line0", "// line1", "// line2", "// line3", "// line4", "// line5", "// line6", "// line7",
        ];
        let block_comment = [
            "/* block0 */",
            "/* block1 */",
            "/* block2 */",
            "/* block3 */",
            "/* block4 */",
            "/* block5 */",
            "/* block6 */",
            "/* block7 */",
            "/* block8 */",
            "/* block9 */",
            "/* block10 */",
        ];
        // let source = "// leading\nfn first() { /* inside */ 1 }\n// between\nfn second() {} // eof\n";
        let source = format!(
            "{}
fn {} first() {} {{
 match {} witness::PATH {} {{
        {} {}
        Left(x: {} (Either<u1, u2>,  {} Either<Option<bool>,  {} (u1, u2, u4)>)) => {{
            let {} ( {} either1, {} either2): {} (Either<u1, u2>, Either<Option<bool>, (u1, u2, u4)>) = x;
        }},
        {}
        Right(x: List<u4, 4>) => {{
            let list1: List<u4, 4> = x;
            {}
        }},
        {}
    }}
}}
 {}
 {}",
            line_comment[0],
            block_comment[0],
            block_comment[1],
            block_comment[2],
            block_comment[3],
            line_comment[6],
            line_comment[7],
            block_comment[4],
            block_comment[5],
            block_comment[6],
            block_comment[7],
            block_comment[8],
            block_comment[9],
            block_comment[10],
            line_comment[1],
            line_comment[2],
            line_comment[3],
            line_comment[4],
            line_comment[5],
        );

        let formatted = dbg!(format(dbg!(&source)));

        for comment in line_comment.iter().chain(block_comment.iter()) {
            assert!(formatted.contains(comment), "missing {comment}");
        }
        assert_eq!(format(&formatted), formatted);
        let source_file = SourceFile::anonymous(Arc::from(formatted.as_str()));
        Program::parse_for_formatting(0, &source_file, &UnstableFeatures::none()).expect("formatted source reparses");
    }

    #[test]
    fn preserves_comment_only_files() {
        let source = "/* outer /* nested */ outer */\r\n// eof\r\n";
        assert_eq!(format(source), source);
    }

    #[test]
    fn preserves_simc_prefix_and_following_comments() {
        let source = "// header\nsimc \"*\";\n// body\nfn main() {}";
        let formatted = format(source);

        assert!(formatted.starts_with("// header\nsimc \"*\";"));
        assert!(formatted.contains("// body"));
        assert_eq!(format(&formatted), formatted);
    }

    #[test]
    fn preserves_blank_lines_between_comment_free_items() {
        let source = "fn one() {}\n\n\nfn two() {}";
        let formatted = format(source);

        assert_eq!(formatted, "fn one() {}\n\nfn two() {}");
    }

    #[test]
    fn removes_whitespace_from_blank_lines() {
        assert_eq!(
            super::remove_whitespace_from_blank_lines("one\n \t\n\t \r\ntwo\n   "),
            "one\n\n\r\ntwo\n"
        );
    }
}

use prettysimf::driver::{EmitMode, FmtConfig, FormatInput, FormatterSession, PartialConfig};
use prettysimf::{FormatOptions, pretty_simf_please};

const SOURCE: &str = "fn main(){assert!(jet::eq_1(param::FLAG,witness::BIT));}";
const FORMATTED_SOURCE: &str = "fn main() {
    assert!(jet::eq_1(param::FLAG, witness::BIT));
}
";

#[test]
fn simple_api_formats_text() {
    let formatted = pretty_simf_please(SOURCE.to_owned(), FormatOptions::default()).unwrap();
    assert_eq!(formatted, FORMATTED_SOURCE);
}

#[test]
fn driver_api_formats_with_shared_configuration() {
    let mut config = FmtConfig::default();
    config.apply_override(PartialConfig {
        emit_mode: Some(EmitMode::Stdout),
        ..PartialConfig::default()
    });
    let mut output = Vec::new();
    {
        let mut session = FormatterSession::new(config, Some(&mut output));

        session.format_and_emit_report(FormatInput::Text(SOURCE.to_owned()));

        assert!(session.has_no_errors());
        assert!(session.used_options().line_width.is_some());
    }
    assert_eq!(String::from_utf8(output).unwrap(), FORMATTED_SOURCE);
}

use std::fs;
use std::path::Path;

use crate::simfmt;

fn assert_fixture(input_file: &str, target_file: &str) {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let input_path = tests_dir.join(input_file);
    let target_path = tests_dir.join(target_file);
    let expected = fs::read_to_string(&target_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", target_path.display()));

    let (exit_status, stdout, stderr) = simfmt(&[
        "--emit",
        "stdout",
        input_path.to_str().expect("fixture path must be valid UTF-8"),
    ]);

    assert!(
        exit_status.success(),
        "simfmt failed for {input_file}\nstdout:\n`{stdout}`\nstderr:\n{stderr}"
    );
    assert!(
        stderr.is_empty(),
        "simfmt wrote diagnostics for {input_file}:\n{stderr}"
    );
    assert_eq!(
        stdout, expected,
        "formatted output did not match, input file: `{input_path:?}`, `{target_file:?}`"
    );
}

fn assert_eq_cmr(
    input_file: &str,
    input_arguments: simplex::simplicityhl::Arguments,
    target_file: &str,
    target_arguments: simplex::simplicityhl::Arguments,
) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let input_path = manifest_dir.join(input_file);
    let target_path = manifest_dir.join(target_file);
    let input_source = fs::read_to_string(&input_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", input_path.display()));
    let target_source = fs::read_to_string(&target_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", target_path.display()));

    assert_eq!(
        get_cmr(input_file, &input_source, input_arguments),
        get_cmr(target_file, &target_source, target_arguments),
        "formatting changed the CMR: `{}` -> `{}`",
        input_path.display(),
        target_path.display(),
    );
}

fn get_cmr(source_name: &str, source: &str, arguments: simplex::simplicityhl::Arguments) -> String {
    use simplex::simplicityhl::ast::ElementsJetHinter;
    use simplex::simplicityhl::{TemplateProgram, UnstableFeatures};

    let template =
        TemplateProgram::new_with_unstable(source, &UnstableFeatures::all(), Box::new(ElementsJetHinter::new()))
            .unwrap_or_else(|error| panic!("failed to compile template `{source_name}`: {error}"));
    let compiled = template
        .instantiate(arguments, false)
        .unwrap_or_else(|error| panic!("failed to instantiate `{source_name}`: {error}"));

    compiled.commit().cmr().to_string()
}

fn assert_failing_fixture(input_file: &str, target_file: &str) {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let input_path = tests_dir.join(input_file);
    let target_path = tests_dir.join(target_file);
    let expected = fs::read_to_string(&target_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", target_path.display()));
    let original = fs::read_to_string(&input_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", input_path.display()));

    assert_eq!(
        original, expected,
        "failing fixture source and target differ: `{input_path:?}`, `{target_path:?}`"
    );

    let (exit_status, stdout, stderr) = simfmt(&[input_path.to_str().expect("fixture path must be valid UTF-8")]);
    let contents = fs::read_to_string(&input_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", input_path.display()));

    assert!(
        !exit_status.success(),
        "simfmt unexpectedly succeeded for {input_file}\nstdout:\n`{stdout}`\nstderr:\n{stderr}"
    );
    assert!(stdout.is_empty(), "unexpected stdout for {input_file}: {stdout}");
    assert!(!stderr.is_empty(), "simfmt failed without diagnostics for {input_file}");
    assert_eq!(
        contents, expected,
        "simfmt modified a failing fixture: `{input_path:?}`, `{target_path:?}`"
    );
}

macro_rules! ui_tests {
    ($($name:ident: $input:literal => $expected:literal,)+) => {
        $(
            #[test]
            fn $name() {
                assert_fixture($input, $expected);
            }
        )+
    };
}

// We wrap arguments type in literal to reduce errors in IDEs
macro_rules! cmr_ui_tests {
    ($($name:ident: $input:tt => $expected:tt, $module:ident, $arguments:literal,)+) => {
        $(
            #[test]
            fn $name() {
                use simplex::program::ArgumentsTrait;

                #[allow(clippy::unused_unit)]
                mod input {
                    simplex::include_simf!($input);
                    paste::paste! {
                        pub(super) type TestArguments = $module::[<$arguments>];
                    }
                }
                #[allow(clippy::unused_unit)]
                mod target {
                    simplex::include_simf!($expected);
                    paste::paste! {
                        pub(super) type TestArguments = $module::[<$arguments>];
                    }
                }

                assert_eq_cmr(
                    $input,
                    input::TestArguments::default().build_arguments(),
                    $expected,
                    target::TestArguments::default().build_arguments(),
                );
            }
        )+
    };
}

macro_rules! failing_ui_tests {
    ($($name:ident: $input:literal => $expected:literal,)+) => {
        $(
            #[test]
            fn $name() {
                assert_failing_fixture($input, $expected);
            }
        )+
    };
}

cmr_ui_tests! {
    cmr_array_tr_storage: "tests/source/real_contracts/array_tr_storage.simf" => "tests/target/real_contracts/array_tr_storage.simf", derived_array_tr_storage, "ArrayTrStorageArguments",
    cmr_bytes32_tr_storage: "tests/source/real_contracts/bytes32_tr_storage.simf" => "tests/target/real_contracts/bytes32_tr_storage.simf", derived_bytes32_tr_storage, "Bytes32TrStorageArguments",
    cmr_dual_currency_deposit: "tests/source/real_contracts/dual_currency_deposit.simf" => "tests/target/real_contracts/dual_currency_deposit.simf", derived_dual_currency_deposit, "DualCurrencyDepositArguments",
    cmr_either_with_single_witness: "tests/source/real_contracts/either_with_single_witness.simf" => "tests/target/real_contracts/either_with_single_witness.simf", derived_either_with_single_witness, "EitherWithSingleWitnessArguments",
    cmr_exotic_values: "tests/source/real_contracts/exotic_values.simf" => "tests/target/real_contracts/exotic_values.simf", derived_exotic_values, "ExoticValuesArguments",
    cmr_option_offer: "tests/source/real_contracts/option_offer.simf" => "tests/target/real_contracts/option_offer.simf", derived_option_offer, "OptionOfferArguments",
    cmr_options: "tests/source/real_contracts/options.simf" => "tests/target/real_contracts/options.simf", derived_options, "OptionsArguments",
    cmr_simple_storage: "tests/source/real_contracts/simple_storage.simf" => "tests/target/real_contracts/simple_storage.simf", derived_simple_storage, "SimpleStorageArguments",
    cmr_single_bit: "tests/source/real_contracts/single_bit.simf" => "tests/target/real_contracts/single_bit.simf", derived_single_bit, "SingleBitArguments",
    cmr_maker_order: "tests/source/real_contracts/maker_order.simf" => "tests/target/real_contracts/maker_order.simf", derived_maker_order, "MakerOrderArguments",
    cmr_prediction_market: "tests/source/real_contracts/prediction_market.simf" => "tests/target/real_contracts/prediction_market.simf", derived_prediction_market, "PredictionMarketArguments",
    cmr_match_arm_blocks: "tests/source/various/match_arm_blocks.simf" => "tests/target/various/match_arm_blocks.simf", derived_match_arm_blocks, "MatchArmBlocksArguments",
    cmr_omit_comas_in_tuples: "tests/source/various/omit_comas_in_tuples.simf" => "tests/target/various/omit_comas_in_tuples.simf", derived_omit_comas_in_tuples, "OmitComasInTuplesArguments",
}

// We can't generate arguments for this contracts:
// (unstable api not yet implemented in simplex)
// * tests/source/real_contracts/list_check.simf
// * tests/source/real_contracts/lmsr_pool.simf
// * tests/source/real_contracts/starkware_symphony.simf
// (don't have main function)
// * tests/source/real_contracts/lmsr_pool.simf

ui_tests! {
    array_tr_storage: "source/real_contracts/array_tr_storage.simf" => "target/real_contracts/array_tr_storage.simf",
    bytes32_tr_storage: "source/real_contracts/bytes32_tr_storage.simf" => "target/real_contracts/bytes32_tr_storage.simf",
    dual_currency_deposit: "source/real_contracts/dual_currency_deposit.simf" => "target/real_contracts/dual_currency_deposit.simf",
    either_with_single_witness: "source/real_contracts/either_with_single_witness.simf" => "target/real_contracts/either_with_single_witness.simf",
    exotic_values: "source/real_contracts/exotic_values.simf" => "target/real_contracts/exotic_values.simf",
    list_check: "source/real_contracts/list_check.simf" => "target/real_contracts/list_check.simf",
    option_offer: "source/real_contracts/option_offer.simf" => "target/real_contracts/option_offer.simf",
    options: "source/real_contracts/options.simf" => "target/real_contracts/options.simf",
    simple_storage: "source/real_contracts/simple_storage.simf" => "target/real_contracts/simple_storage.simf",
    single_bit: "source/real_contracts/single_bit.simf" => "target/real_contracts/single_bit.simf",
    lmsr_pool: "source/real_contracts/lmsr_pool.simf" => "target/real_contracts/lmsr_pool.simf",
    maker_order: "source/real_contracts/maker_order.simf" => "target/real_contracts/maker_order.simf",
    prediction_market: "source/real_contracts/prediction_market.simf" => "target/real_contracts/prediction_market.simf",
    starkware_symphony: "source/real_contracts/starkware_symphony.simf" => "target/real_contracts/starkware_symphony.simf",
    match_arm_blocks: "source/various/match_arm_blocks.simf" => "target/various/match_arm_blocks.simf",
    omit_comas_in_tuples: "source/various/omit_comas_in_tuples.simf" => "target/various/omit_comas_in_tuples.simf",
    comment_trivia: "source/various/comment_trivia.simf" => "target/various/comment_trivia.simf",
}

failing_ui_tests! {
    unsupported_comment: "source/errorneous/unsupported_comment.simf" => "target/errorneous/unsupported_comment.simf",
}

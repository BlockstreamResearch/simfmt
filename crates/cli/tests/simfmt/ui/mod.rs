use crate::simfmt;
use std::fs;
use std::path::Path;

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
}

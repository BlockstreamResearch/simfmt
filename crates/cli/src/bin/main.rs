use tracing_subscriber::EnvFilter;

use std::io::Write;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("SIMFMT_LOG"))
        .init();
    let opts = simfmt::cli::make_opts();

    let exit_code = match simfmt::cli::execute(&opts) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    };
    // Make sure standard output is flushed before we exit.
    std::io::stdout().flush().unwrap();

    // Exit with given exit code.
    //
    // NOTE: this immediately terminates the process without doing any cleanup,
    // so make sure to finish all necessary cleanup before this is called.
    std::process::exit(exit_code);
}

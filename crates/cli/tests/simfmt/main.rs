use std::env;
use std::path::Path;
use std::process::{Command, ExitStatus};

#[path = "ui/mod.rs"]
mod ui;

fn simfmt_with_extra(args: &[&str], working_dir: Option<&str>, envs: &[(&str, &str)]) -> (ExitStatus, String, String) {
    let simfmt = env!("CARGO_BIN_EXE_simfmt");
    let bin_dir = Path::new(simfmt).parent().unwrap();

    // Ensure the simfmt binary runs from the local target dir.
    let path = env::var_os("PATH").unwrap_or_default();
    let mut paths = env::split_paths(&path).collect::<Vec<_>>();
    paths.insert(0, bin_dir.to_owned());
    let new_path = env::join_paths(paths).unwrap();
    let mut cmd = Command::new(simfmt);
    cmd.args(args).env("PATH", new_path).envs(envs.iter().copied());
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
    match cmd.output() {
        Ok(output) => (
            output.status,
            String::from_utf8(output.stdout).expect("utf-8"),
            String::from_utf8(output.stderr).expect("utf-8"),
        ),
        Err(e) => panic!("failed to run `{cmd:?} {args:?}`: {e}"),
    }
}

fn simfmt(args: &[&str]) -> (ExitStatus, String, String) {
    simfmt_with_extra(args, None, &[])
}

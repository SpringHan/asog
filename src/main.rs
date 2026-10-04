//! 命令行程序入口，具体逻辑见库中的 [`asog::run_cli`]。

use std::process::ExitCode;

fn main() -> ExitCode {
    asog::run_cli()
}

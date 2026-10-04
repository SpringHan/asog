//! asog —— 把 Markdown 文件夹编译为完整的静态博客站点。
//!
//! 模块划分对应 `docs/MAIN.md` 中的要求：
//!
//! * [`cli`]：基于 `clap` 的命令行参数解析（`-f/--from`、`-o/--output`）。
//! * [`markdown`]：基于 `markdown` crate 的 Markdown → HTML 编译。
//! * [`page`] + [`site::DEFAULT_STYLE`]：网页模板与样式表。
//! * [`frontmatter`]：文章元数据（标题、日期、标签、草稿等）。
//! * [`site`]：站点构建流水线。
//! * [`util`]：转义、slug、路径等通用工具。

pub mod cli;
pub mod frontmatter;
pub mod markdown;
pub mod page;
pub mod site;
pub mod util;

pub use cli::Cli;
pub use site::{Config, Report, build, format_report};

use std::process::ExitCode;

use clap::Parser;

/// 命令行入口：解析参数、执行构建并输出结果。
///
/// 返回进程退出码，便于 `main` 直接返回。
pub fn run_cli() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose > 0;
    let config = Config::from(cli);

    match site::build(&config) {
        Ok(report) => {
            print!("{}", site::format_report(&report, verbose));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("构建失败：{error:#}");
            ExitCode::FAILURE
        }
    }
}

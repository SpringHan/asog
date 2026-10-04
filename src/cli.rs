//! 命令行参数定义（基于 `clap` 的 derive API）。

use std::path::PathBuf;

use clap::Parser;

/// 把 Markdown 文件夹编译为完整的静态博客站点。
#[derive(Debug, Clone, Parser)]
#[command(
    name = "asog",
    version,
    about = "把 Markdown 文件夹编译为完整的静态博客站点",
    long_about = "asog 会递归读取源目录中的 Markdown 文件，将其编译为 HTML 页面，\
并生成首页、标签页，同时把图片等静态资源复制到输出目录。\n\n\
示例：\n  asog -f ./content -o ./public --title \"我的博客\""
)]
pub struct Cli {
    /// 所要获取的 markdown 文件的路径（源目录）
    #[arg(short = 'f', long = "from", value_name = "PATH")]
    pub from: PathBuf,

    /// 最终输出静态网页所有文件的路径（输出目录）
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    pub output: PathBuf,

    /// 站点标题，显示在页眉与浏览器标签页上
    #[arg(long, value_name = "TITLE", default_value = "Asog Blog")]
    pub title: String,

    /// 站点描述，写入首页与 `<meta name="description">`
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    /// 作者名，显示在页脚
    #[arg(long, value_name = "NAME")]
    pub author: Option<String>,

    /// 页面语言，写入 `<html lang="...">`
    #[arg(long, value_name = "LANG", default_value = "zh-CN")]
    pub lang: String,

    /// 构建前清空输出目录
    #[arg(long)]
    pub clean: bool,

    /// 同时构建 `draft: true` 的草稿
    #[arg(long)]
    pub drafts: bool,

    /// 禁止 Markdown 中的原始 HTML（默认允许，便于插入自定义标签）
    #[arg(long)]
    pub no_raw_html: bool,

    /// 输出每个文件的详细构建日志（可重复以显示更多信息）
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_short_and_long_flags() {
        let cli = Cli::try_parse_from(["asog", "-f", "content", "-o", "public"]).unwrap();
        assert_eq!(cli.from, PathBuf::from("content"));
        assert_eq!(cli.output, PathBuf::from("public"));
        assert_eq!(cli.title, "Asog Blog");
        assert!(!cli.clean);

        let cli = Cli::try_parse_from([
            "asog",
            "--from",
            "src",
            "--output",
            "dist",
            "--title",
            "我的博客",
            "--clean",
            "--drafts",
            "-vv",
        ])
        .unwrap();
        assert_eq!(cli.from, PathBuf::from("src"));
        assert_eq!(cli.title, "我的博客");
        assert!(cli.clean && cli.drafts);
        assert_eq!(cli.verbose, 2);
    }

    #[test]
    fn requires_from_and_output() {
        assert!(Cli::try_parse_from(["asog"]).is_err());
        assert!(Cli::try_parse_from(["asog", "-f", "content"]).is_err());
    }
}

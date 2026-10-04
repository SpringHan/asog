//! 端到端测试：通过库 API 构建示例站点，并以命令行程序验证 CLI 行为。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use asog::Config;

/// 基于 `CARGO_TARGET_TMPDIR` 的临时目录，测试结束自动清理。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("{tag}-{}-{unique}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn example_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("example/content")
}

fn example_config(output: &Path) -> Config {
    Config {
        from: example_source(),
        output: output.to_path_buf(),
        title: "asog 示例站点".to_string(),
        description: "示例描述".to_string(),
        author: "示例作者".to_string(),
        lang: "zh-CN".to_string(),
        clean: false,
        drafts: false,
        raw_html: true,
        verbose: false,
    }
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("读取 {} 失败：{error}", path.display()))
}

#[test]
fn builds_complete_example_site() {
    let temp = TempDir::new("example");
    let report = asog::build(&example_config(temp.path())).expect("构建示例站点");

    // 首页、文章页、标签页与静态资源都应生成。
    for relative in [
        "index.html",
        "style.css",
        "hello-world.html",
        "about.html",
        "rust/ownership.html",
        "tags/rust.html",
        "tags/静态站点.html",
        "tags/index.html",
        "assets/logo.svg",
    ] {
        let path = temp.path().join(relative);
        assert!(path.is_file(), "缺少输出文件：{}", path.display());
    }
    // 草稿默认不构建。
    assert!(!temp.path().join("draft-post.html").exists());
    assert_eq!(report.drafts_skipped, 1);
    assert_eq!(report.posts, 3);
    assert_eq!(report.assets, 1);

    // 首页：站点标题、文章链接与标签云。
    let index = read(&temp.path().join("index.html"));
    assert!(index.contains("<title>asog 示例站点</title>"));
    assert!(index.contains("href=\"hello-world.html\""));
    assert!(index.contains("href=\"rust/ownership.html\""));
    assert!(index.contains("class=\"tag-cloud\""));
    // 列表中的文章从 Markdown 一级标题推导标题。
    assert!(index.contains(">所有权<"));
    // `listed: false` 的页面不进入首页文章列表……
    assert!(!index.contains("post-card-title\"><a href=\"about.html\""));
    // ……但它设置了 title_bar: true，因此出现在页眉导航中，并可标记当前页。
    assert!(index.contains("<a href=\"about.html\">关于本站</a>"));
    assert!(
        index.contains("<a class=\"is-active\" href=\"index.html\" aria-current=\"page\">首页</a>")
    );
    // 导航顺序：首页 → title_bar 页面 → 标签。
    let home_at = index.find(">首页</a>").expect("首页导航项");
    let about_at = index.find(">关于本站</a>").expect("关于本站导航项");
    let tags_at = index.find(">标签</a>").expect("标签导航项");
    assert!(home_at < about_at && about_at < tags_at);
    // 首页正文来自 index.md。
    assert!(index.contains("这是 **asog** 的示例站点。") || index.contains("示例站点"));

    // 文章页：目录、锚点、日期与标签。
    let post = read(&temp.path().join("hello-world.html"));
    assert!(post.contains("<title>你好，世界 · asog 示例站点</title>"));
    assert!(post.contains("datetime=\"2025-01-02T10:30:00\""));
    assert!(post.contains("class=\"toc\""));
    assert!(post.contains("<h2 id=\"支持的语法\">"));
    assert!(post.contains("<h2 id=\"代码块\">"));
    assert!(post.contains("href=\"#支持的语法\""));
    assert!(post.contains("href=\"tags/rust.html\""));
    // 标题 id 必须是标签属性，不能泄漏成正文文本。
    assert!(!post.contains("> id="));

    // 嵌套目录中的页面使用相对前缀引用站点资源，导航链接同样带前缀。
    let nested = read(&temp.path().join("rust/ownership.html"));
    assert!(nested.contains("href=\"../style.css\""));
    assert!(nested.contains("href=\"../index.html\""));
    assert!(nested.contains("href=\"../about.html\""));
    assert!(nested.contains("<h1 class=\"post-title\">所有权</h1>"));
    assert!(nested.contains("src=\"../assets/logo.svg\""));

    // 标签总览页与内置样式表。
    let tags = read(&temp.path().join("tags/index.html"));
    assert!(tags.contains("href=\"../tags/%E9%9D%99%E6%80%81%E7%AB%99%E7%82%B9.html\""));
    // 标签页也应包含 title_bar 导航项，并把自己标记为当前页。
    assert!(tags.contains("<a href=\"../about.html\">关于本站</a>"));
    assert!(tags.contains(
        "<a class=\"is-active\" href=\"../tags/index.html\" aria-current=\"page\">标签</a>"
    ));
    let style = read(&temp.path().join("style.css"));
    assert!(style.contains("--accent"));
}

#[test]
fn includes_drafts_when_requested() {
    let temp = TempDir::new("drafts");
    let mut config = example_config(temp.path());
    config.drafts = true;
    let report = asog::build(&config).expect("构建含草稿的站点");

    assert!(temp.path().join("draft-post.html").is_file());
    assert_eq!(report.drafts_skipped, 0);
    assert_eq!(report.posts, 4);
}

#[test]
fn clean_removes_stale_output() {
    let temp = TempDir::new("clean");
    asog::build(&example_config(temp.path())).expect("首次构建");

    let stale = temp.path().join("stale.html");
    fs::write(&stale, "旧的输出").expect("写入过期文件");
    assert!(stale.exists());

    let mut config = example_config(temp.path());
    config.clean = true;
    asog::build(&config).expect("清空后重建");

    assert!(!stale.exists(), "开启 --clean 后应删除过期文件");
    assert!(temp.path().join("index.html").is_file());
}

#[test]
fn custom_stylesheet_takes_precedence() {
    let temp = TempDir::new("style");
    let source = temp.path().join("content");
    fs::create_dir_all(&source).expect("创建源目录");
    fs::write(source.join("post.md"), "# 标题\n\n正文\n").expect("写入 Markdown");
    fs::write(source.join("style.css"), "/* 自定义样式 */\n").expect("写入样式表");

    let mut config = example_config(&temp.path().join("out"));
    config.from = source;
    asog::build(&config).expect("构建站点");

    let style = read(&temp.path().join("out/style.css"));
    assert_eq!(style, "/* 自定义样式 */\n");
}

#[test]
fn nested_directories_and_assets_are_preserved() {
    let temp = TempDir::new("nested");
    let source = temp.path().join("content");
    fs::create_dir_all(source.join("guide/deep")).expect("创建源目录");
    fs::write(source.join("guide/deep/page.md"), "# 深层页面\n").expect("写入 Markdown");
    fs::write(source.join("guide/note.txt"), "静态文本").expect("写入静态文件");
    // 隐藏目录应被忽略。
    fs::create_dir_all(source.join(".git")).expect("创建隐藏目录");
    fs::write(source.join(".git/config"), "[]").expect("写入隐藏文件");

    let mut config = example_config(&temp.path().join("out"));
    config.from = source;
    asog::build(&config).expect("构建站点");

    assert!(temp.path().join("out/guide/deep/page.html").is_file());
    assert_eq!(read(&temp.path().join("out/guide/note.txt")), "静态文本");
    assert!(!temp.path().join("out/.git").exists());

    let page = read(&temp.path().join("out/guide/deep/page.html"));
    assert!(page.contains("href=\"../../style.css\""));
}

#[test]
fn rejects_output_inside_source() {
    let temp = TempDir::new("reject");
    let source = temp.path().join("content");
    fs::create_dir_all(&source).expect("创建源目录");

    let mut config = example_config(&source.join("public"));
    config.from = source;
    let error = asog::build(&config).expect_err("输出目录位于源目录内部应报错");
    assert!(format!("{error:#}").contains("输出目录不能位于源目录内部"));
}

#[test]
fn reports_missing_source_directory() {
    let temp = TempDir::new("missing");
    let mut config = example_config(&temp.path().join("out"));
    config.from = temp.path().join("不存在的目录");

    let error = asog::build(&config).expect_err("源目录缺失应报错");
    assert!(format!("{error:#}").contains("源目录不存在"));
}

#[test]
fn cli_help_documents_required_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_asog"))
        .arg("--help")
        .output()
        .expect("运行 asog --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("-f, --from <PATH>"));
    assert!(stdout.contains("-o, --output <PATH>"));
}

#[test]
fn cli_requires_from_and_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_asog"))
        .output()
        .expect("运行 asog");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--from") || stderr.contains("--output"));
}

#[test]
fn cli_builds_site_from_flags() {
    let temp = TempDir::new("cli");
    let output = Command::new(env!("CARGO_BIN_EXE_asog"))
        .arg("-f")
        .arg(example_source())
        .arg("-o")
        .arg(temp.path())
        .args(["--title", "CLI 站点", "--author", "测试"])
        .output()
        .expect("运行 asog");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("构建完成"));
    let index = read(&temp.path().join("index.html"));
    assert!(index.contains("<title>CLI 站点</title>"));
    assert!(index.contains("测试"));
}

#[test]
fn cli_reports_failure_for_missing_source() {
    let temp = TempDir::new("cli-fail");
    let output = Command::new(env!("CARGO_BIN_EXE_asog"))
        .arg("-f")
        .arg(temp.path().join("missing"))
        .arg("-o")
        .arg(temp.path().join("out"))
        .output()
        .expect("运行 asog");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("构建失败"));
}

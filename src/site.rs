//! 站点构建流水线：发现源文件 → 编译 Markdown → 渲染页面 → 复制资源。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::cli::Cli;
use crate::frontmatter::{self, Date};
use crate::markdown::{self, RenderOptions, TocEntry};
use crate::page::{self, NavItem, PostLink, PostView, Site, TagLink};
use crate::util;

/// 内置样式表，当源目录未提供 `style.css` 时写入输出目录。
pub const DEFAULT_STYLE: &str = include_str!("assets/style.css");

/// 构建配置。
#[derive(Debug, Clone)]
pub struct Config {
    /// Markdown 源目录。
    pub from: PathBuf,
    /// 静态站点输出目录。
    pub output: PathBuf,
    /// 站点标题。
    pub title: String,
    /// 站点描述。
    pub description: String,
    /// 作者名。
    pub author: String,
    /// 页面语言。
    pub lang: String,
    /// 构建前是否清空输出目录。
    pub clean: bool,
    /// 是否包含草稿。
    pub drafts: bool,
    /// 是否允许 Markdown 中的原始 HTML。
    pub raw_html: bool,
    /// 是否输出详细日志。
    pub verbose: bool,
}

impl From<Cli> for Config {
    fn from(cli: Cli) -> Self {
        Self {
            from: cli.from,
            output: cli.output,
            title: cli.title,
            description: cli.description.unwrap_or_default(),
            author: cli.author.unwrap_or_default(),
            lang: cli.lang,
            clean: cli.clean,
            drafts: cli.drafts,
            raw_html: !cli.no_raw_html,
            verbose: cli.verbose > 0,
        }
    }
}

/// 构建结果统计。
#[derive(Debug, Default, Clone)]
pub struct Report {
    /// 生成的 HTML 文章 / 页面数量（不含首页与标签页）。
    pub posts: usize,
    /// 生成的标签页数量（含标签总览页）。
    pub tag_pages: usize,
    /// 复制的静态资源数量。
    pub assets: usize,
    /// 因 `draft: true` 而跳过的文件数量。
    pub drafts_skipped: usize,
    /// 输出目录（绝对路径）。
    pub output: PathBuf,
    /// 需要提示给用户的问题（始终输出）。
    pub warnings: Vec<String>,
    /// 构建动作明细（`--verbose` 时输出）。
    pub actions: Vec<String>,
}

/// 执行一次完整构建。
pub fn build(config: &Config) -> Result<Report> {
    let from = config
        .from
        .canonicalize()
        .with_context(|| format!("源目录不存在或不可访问：{}", config.from.display()))?;
    if !from.is_dir() {
        bail!("源路径不是目录：{}", from.display());
    }

    let output = util::normalize_path(&util::absolute(&config.output));
    if output.starts_with(&from) {
        bail!(
            "输出目录不能位于源目录内部（源：{}，输出：{}），请改用其他位置",
            from.display(),
            output.display()
        );
    }

    if config.clean && output.exists() {
        ensure_cleanable(&output)?;
        fs::remove_dir_all(&output)
            .with_context(|| format!("无法清空输出目录：{}", output.display()))?;
    }
    fs::create_dir_all(&output)
        .with_context(|| format!("无法创建输出目录：{}", output.display()))?;

    let mut report = Report {
        output: output.clone(),
        ..Report::default()
    };
    let render_options = RenderOptions {
        raw_html: config.raw_html,
    };

    let (sources, assets) = collect(&from)?;
    report.actions.push(format!(
        "发现 {} 个 Markdown 文件、{} 个静态资源",
        sources.len(),
        assets.len()
    ));

    // 1. 编译所有 Markdown 源文件。
    let mut pages: Vec<Page> = Vec::new();
    let mut home: Option<Page> = None;
    for (rel, abs) in &sources {
        match compile_page(rel, abs, &render_options, config, &mut report)? {
            Some(page) if is_home(rel) => {
                if page.title_bar {
                    report.warnings.push(format!(
                        "{}：首页已固定出现在页眉导航中，title_bar 被忽略",
                        util::rel_url(rel)
                    ));
                }
                home = Some(page);
            }
            Some(page) => pages.push(page),
            None => {}
        }
    }
    pages.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));

    // 2. 汇总标签、站点图标与页眉导航（导航按源文件路径排序，便于用文件名控制顺序）。
    let icon = detect_icon(&assets);
    if let Some(icon) = &icon {
        report.actions.push(format!("使用 {icon} 作为站点图标"));
    } else {
        report
            .actions
            .push("未找到 assets/logo.*，站点不设置图标".to_string());
    }
    let tags = collect_tags(&pages);
    let mut title_bar_pages: Vec<&Page> = pages.iter().filter(|page| page.title_bar).collect();
    title_bar_pages.sort_by(|a, b| a.source.cmp(&b.source));
    let nav: Vec<NavItem> = title_bar_pages
        .iter()
        .map(|page| NavItem {
            title: page.title.clone(),
            url: util::rel_url(&page.rel_out),
        })
        .collect();
    let site = Site {
        title: &config.title,
        description: &config.description,
        author: &config.author,
        lang: &config.lang,
        has_tags: !tags.is_empty(),
        nav: &nav,
        icon: icon.as_deref(),
    };

    // 3. 渲染文章页。
    for page in &pages {
        let base = util::relative_prefix(&page.rel_out);
        let view = PostView {
            title: &page.title,
            date: page.date,
            updated: page.updated,
            tags: inline_tags(&page.tags, &base, &tags),
            content: &page.content,
            toc: if page.toc { &page.toc_entries } else { &[] },
        };
        let current = util::rel_url(&page.rel_out);
        let html = page::post(&site, &base, &current, &view);
        write_text(
            &output.join(&page.rel_out),
            &html,
            &mut report,
            &page.rel_out,
        )?;
        report.posts += 1;
    }

    // 4. 渲染首页。
    let home_body = home
        .as_ref()
        .map(|page| page.content.as_str())
        .unwrap_or_default();
    let home_title = home.as_ref().map(|page| page.title.as_str());
    let posts: Vec<PostLink<'_>> = pages
        .iter()
        .filter(|page| page.listed)
        .map(|page| post_link(page, "", &tags))
        .collect();
    let all_tags = cloud_links(&tags, "");
    let html = page::index(&site, "", home_title, home_body, &posts, &all_tags);
    write_text(
        &output.join("index.html"),
        &html,
        &mut report,
        Path::new("index.html"),
    )?;

    // 5. 渲染标签页与标签总览页。
    for (slug, group) in &tags {
        let rel_out = PathBuf::from("tags").join(format!("{slug}.html"));
        let base = util::relative_prefix(&rel_out);
        let posts: Vec<PostLink<'_>> = group
            .posts
            .iter()
            .map(|index| post_link(&pages[*index], &base, &tags))
            .collect();
        let current = util::rel_url(&rel_out);
        let html = page::tag_page(&site, &base, &current, &group.name, &posts);
        write_text(&output.join(&rel_out), &html, &mut report, &rel_out)?;
        report.tag_pages += 1;
    }
    if !tags.is_empty() {
        let rel_out = PathBuf::from("tags/index.html");
        let base = util::relative_prefix(&rel_out);
        let html = page::tags_page(&site, &base, &cloud_links(&tags, &base));
        write_text(&output.join(&rel_out), &html, &mut report, &rel_out)?;
        report.tag_pages += 1;
    }

    // 6. 复制静态资源（图片、字体、自定义样式等）。
    for (rel, abs) in &assets {
        let destination = output.join(rel);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("无法创建目录：{}", parent.display()))?;
        }
        fs::copy(abs, &destination).with_context(|| {
            format!(
                "复制资源失败：{} → {}",
                abs.display(),
                destination.display()
            )
        })?;
        report.assets += 1;
        report.actions.push(format!("复制 {}", util::rel_url(rel)));
    }

    // 7. 写入内置样式表（源目录提供同名文件时以其为准）。
    let style_path = output.join("style.css");
    if style_path.exists() {
        report
            .actions
            .push("检测到源目录自带 style.css，跳过内置样式表".to_string());
    } else {
        write_text(
            &style_path,
            DEFAULT_STYLE,
            &mut report,
            Path::new("style.css"),
        )?;
    }

    Ok(report)
}

/// 把构建结果格式化为可直接打印的文本。
pub fn format_report(report: &Report, verbose: bool) -> String {
    let mut out = String::new();
    for warning in &report.warnings {
        out.push_str(&format!("警告：{warning}\n"));
    }
    if verbose {
        for action in &report.actions {
            out.push_str(&format!("  · {action}\n"));
        }
    }
    out.push_str(&format!(
        "构建完成：{} 个页面，{} 个标签页，{} 个静态资源",
        report.posts, report.tag_pages, report.assets
    ));
    if report.drafts_skipped > 0 {
        out.push_str(&format!("，跳过 {} 篇草稿", report.drafts_skipped));
    }
    out.push_str(&format!("\n输出目录：{}\n", report.output.display()));
    out
}

/// 一个已编译的页面。
#[derive(Debug)]
struct Page {
    /// 源文件相对路径，用于稳定排序（页眉导航顺序）。
    source: PathBuf,
    rel_out: PathBuf,
    title: String,
    date: Option<Date>,
    updated: Option<Date>,
    tags: Vec<String>,
    summary: String,
    listed: bool,
    toc: bool,
    /// 是否加入页眉导航。
    title_bar: bool,
    content: String,
    toc_entries: Vec<TocEntry>,
}

/// 一个标签及其文章下标。
#[derive(Debug)]
struct TagGroup {
    name: String,
    posts: Vec<usize>,
}

type TagMap = BTreeMap<String, TagGroup>;

/// 一个源文件：`(相对路径, 绝对路径)`。
type FileEntry = (PathBuf, PathBuf);

/// `collect` 的返回值：Markdown 源文件与静态资源。
type Collected = (Vec<FileEntry>, Vec<FileEntry>);

/// 递归收集源目录中的 Markdown 文件与静态资源。
fn collect(from: &Path) -> Result<Collected> {
    let mut sources = Vec::new();
    let mut assets = Vec::new();

    let mut walker = WalkDir::new(from)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry.with_context(|| format!("遍历源目录失败：{}", from.display()))?;
        let Ok(rel) = entry.path().strip_prefix(from) else {
            continue;
        };
        let rel = rel.to_path_buf();
        if rel.as_os_str().is_empty() {
            continue;
        }
        if util::is_hidden(&rel) {
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if entry.file_type().is_dir() {
            continue;
        }
        // `follow_links(false)` 时符号链接既不是文件也不是目录，这里单独判断。
        if entry.path().is_dir() {
            walker.skip_current_dir();
            continue;
        }

        let absolute = entry.path().to_path_buf();
        if is_markdown(&rel) {
            sources.push((rel, absolute));
        } else {
            assets.push((rel, absolute));
        }
    }

    Ok((sources, assets))
}

/// 站点图标的候选文件名，按优先级排列。
///
/// 同时存在多个时优先使用矢量图，其次才是位图。
const ICON_CANDIDATES: [&str; 8] = [
    "assets/logo.svg",
    "assets/logo.png",
    "assets/logo.ico",
    "assets/logo.webp",
    "assets/logo.avif",
    "assets/logo.jpg",
    "assets/logo.jpeg",
    "assets/logo.gif",
];

/// 在已收集的静态资源中查找 `assets/logo.*` 作为站点图标。
///
/// 名称匹配不区分大小写，返回值保留源目录中的真实大小写；
/// 找不到时返回 `None`，此时生成的页面不含任何 icon 声明。
fn detect_icon(assets: &[FileEntry]) -> Option<String> {
    for candidate in ICON_CANDIDATES {
        let found = assets.iter().find(|(rel, _)| {
            rel.to_string_lossy()
                .replace('\\', "/")
                .eq_ignore_ascii_case(candidate)
        });
        if let Some((rel, _)) = found {
            return Some(util::rel_url(rel));
        }
    }
    None
}

/// 编译单个 Markdown 文件；草稿返回 `None`。
fn compile_page(
    rel: &Path,
    abs: &Path,
    options: &RenderOptions,
    config: &Config,
    report: &mut Report,
) -> Result<Option<Page>> {
    let source = fs::read_to_string(abs)
        .with_context(|| format!("无法以 UTF-8 读取文件：{}", rel.display()))?;
    let (front, body) = frontmatter::parse(&source);
    for warning in &front.warnings {
        report
            .warnings
            .push(format!("{}：{warning}", rel.display()));
    }

    if front.draft && !config.drafts {
        report.drafts_skipped += 1;
        report
            .actions
            .push(format!("跳过草稿 {}", util::rel_url(rel)));
        return Ok(None);
    }

    let explicit_title = front.title.clone().filter(|title| !title.trim().is_empty());
    let derived_title = if explicit_title.is_some() {
        None
    } else {
        markdown::first_heading(body)
    };
    let body = if explicit_title.is_none() && derived_title.is_some() {
        // 标题已被提取为文章标题，避免正文中重复出现。
        markdown::strip_first_heading(body)
    } else {
        body
    };

    let rendered = markdown::render(body, options)
        .with_context(|| format!("编译 Markdown 失败：{}", rel.display()))?;

    let title = explicit_title
        .or(derived_title)
        .unwrap_or_else(|| util::title_from_filename(rel));
    let summary = front
        .summary
        .filter(|summary| !summary.trim().is_empty())
        .unwrap_or_else(|| markdown::first_paragraph(&rendered.html));

    Ok(Some(Page {
        source: rel.to_path_buf(),
        rel_out: output_path(rel, front.slug.as_deref()),
        title,
        date: front.date,
        updated: front.updated,
        tags: dedup_tags(front.tags),
        summary,
        listed: front.listed,
        toc: front.toc,
        title_bar: front.title_bar,
        content: rendered.html,
        toc_entries: rendered.toc,
    }))
}

/// 由源文件相对路径推算输出文件相对路径（`.md` → `.html`，支持 `slug`）。
fn output_path(rel: &Path, slug: Option<&str>) -> PathBuf {
    let mut out = rel.to_path_buf();
    let slug = slug.map(util::slugify).filter(|slug| !slug.is_empty());
    match slug {
        Some(slug) => {
            out.set_file_name(format!("{slug}.html"));
        }
        None => {
            out.set_extension("html");
        }
    }
    out
}

fn dedup_tags(tags: Vec<String>) -> Vec<String> {
    let mut seen = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if !tag.is_empty() && !seen.contains(&tag) {
            seen.push(tag);
        }
    }
    seen
}

/// 汇总所有标签；文章下标对 `pages` 有效。
fn collect_tags(pages: &[Page]) -> TagMap {
    let mut map: TagMap = BTreeMap::new();
    for (index, page) in pages.iter().enumerate() {
        if !page.listed {
            continue;
        }
        for tag in &page.tags {
            let slug = tag_slug(tag);
            map.entry(slug)
                .or_insert_with(|| TagGroup {
                    name: tag.clone(),
                    posts: Vec::new(),
                })
                .posts
                .push(index);
        }
    }
    map
}

fn tag_slug(tag: &str) -> String {
    let slug = util::slugify(tag);
    if slug.is_empty() {
        "tag".to_string()
    } else {
        slug
    }
}

/// 文章页中展示的标签链接（带数量）。
fn inline_tags(tags: &[String], base: &str, map: &TagMap) -> Vec<TagLink> {
    tags.iter()
        .map(|tag| {
            let slug = tag_slug(tag);
            TagLink {
                name: tag.clone(),
                url: util::encode_uri_path(&format!("{base}tags/{slug}.html")),
                count: map.get(&slug).map(|group| group.posts.len()).unwrap_or(0),
            }
        })
        .collect()
}

/// 供标签总览 / 首页标签云使用的链接列表。
fn cloud_links(map: &TagMap, base: &str) -> Vec<TagLink> {
    map.iter()
        .map(|(slug, group)| TagLink {
            name: group.name.clone(),
            url: util::encode_uri_path(&format!("{base}tags/{slug}.html")),
            count: group.posts.len(),
        })
        .collect()
}

/// 首页 / 标签页列表中的一篇文章。
fn post_link<'a>(page: &'a Page, base: &str, map: &TagMap) -> PostLink<'a> {
    PostLink {
        title: &page.title,
        url: format!("{base}{}", util::rel_url(&page.rel_out)),
        date: page.date,
        updated: page.updated,
        summary: &page.summary,
        tags: inline_tags(&page.tags, base, map),
    }
}

fn is_home(rel: &Path) -> bool {
    rel.components().count() == 1
        && rel.file_name().is_some_and(|name| {
            let name = name.to_string_lossy();
            name.eq_ignore_ascii_case("index.md") || name.eq_ignore_ascii_case("index.markdown")
        })
}

fn is_markdown(rel: &Path) -> bool {
    rel.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

fn ensure_cleanable(path: &Path) -> Result<()> {
    let names = path
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count();
    if names == 0 {
        bail!("拒绝清空危险路径：{}", path.display());
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    if cwd.starts_with(path) {
        bail!("拒绝清空包含当前工作目录的路径：{}", path.display());
    }
    Ok(())
}

fn write_text(path: &Path, contents: &str, report: &mut Report, label: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("无法创建目录：{}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("写入文件失败：{}", path.display()))?;
    report
        .actions
        .push(format!("生成 {}", util::rel_url(label)));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(rel_out: &str, slug: Option<&str>) -> PathBuf {
        output_path(Path::new(rel_out), slug)
    }

    fn asset(path: &str) -> FileEntry {
        (PathBuf::from(path), PathBuf::from("/source").join(path))
    }

    #[test]
    fn detects_site_icon_by_priority_and_case() {
        // 同时存在时优先矢量图。
        let both = vec![asset("assets/logo.png"), asset("assets/logo.svg")];
        assert_eq!(detect_icon(&both).as_deref(), Some("assets/logo.svg"));

        let only_png = vec![asset("posts/a.md"), asset("assets/logo.png")];
        assert_eq!(detect_icon(&only_png).as_deref(), Some("assets/logo.png"));

        let ico = vec![asset("assets/logo.ico")];
        assert_eq!(detect_icon(&ico).as_deref(), Some("assets/logo.ico"));

        // 大小写不敏感，但保留源目录中的真实文件名。
        let upper = vec![asset("assets/Logo.PNG")];
        assert_eq!(detect_icon(&upper).as_deref(), Some("assets/Logo.PNG"));
    }

    #[test]
    fn skips_files_that_are_not_site_icons() {
        for assets in [
            vec![asset("assets/cover.png")],
            vec![asset("logo.svg")],
            vec![asset("assets/logo.txt")],
            vec![asset("other/assets/logo.svg")],
            Vec::new(),
        ] {
            assert_eq!(detect_icon(&assets), None, "不应匹配：{assets:?}");
        }
    }

    #[test]
    fn maps_markdown_to_html() {
        assert_eq!(page("post.md", None), PathBuf::from("post.html"));
        assert_eq!(
            page("notes/rust/ownership.markdown", None),
            PathBuf::from("notes/rust/ownership.html")
        );
        assert_eq!(page("index.md", None), PathBuf::from("index.html"));
    }

    #[test]
    fn honours_slug_and_rejects_unsafe_paths() {
        assert_eq!(
            page("post.md", Some("my-post")),
            PathBuf::from("my-post.html")
        );
        assert_eq!(
            page("notes/post.md", Some("自定义 标题")),
            PathBuf::from("notes/自定义-标题.html")
        );
        // 含路径分隔符的 slug 会被清理，无法逃出所在目录。
        assert_eq!(
            page("notes/post.md", Some("../../etc/passwd")),
            PathBuf::from("notes/etc-passwd.html")
        );
        assert_eq!(page("post.md", Some("  ")), PathBuf::from("post.html"));
    }

    #[test]
    fn detects_home_and_markdown_files() {
        assert!(is_home(Path::new("index.md")));
        assert!(is_home(Path::new("INDEX.MD")));
        assert!(!is_home(Path::new("posts/index.md")));
        assert!(is_markdown(Path::new("a/b.MD")));
        assert!(!is_markdown(Path::new("a/b.txt")));
    }

    #[test]
    fn dedups_tags() {
        let tags = vec![
            " rust ".to_string(),
            "rust".to_string(),
            String::new(),
            "博客".to_string(),
        ];
        assert_eq!(
            dedup_tags(tags),
            vec!["rust".to_string(), "博客".to_string()]
        );
    }

    #[test]
    fn refuses_to_clean_dangerous_paths() {
        assert!(ensure_cleanable(Path::new("/")).is_err());
        let cwd = std::env::current_dir().unwrap();
        assert!(ensure_cleanable(&cwd).is_err());
        assert!(ensure_cleanable(&cwd.join("some-output")).is_ok());
    }
}

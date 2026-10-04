//! HTML 模板：站点外壳、文章页、首页、标签页与标签总览页。
//!
//! 模板以纯函数方式实现，页面内容由 `site` 模块传入，避免引入模板引擎依赖。

use crate::frontmatter::Date;
use crate::markdown::TocEntry;
use crate::util::escape_html as esc;

/// 站点级信息。
pub struct Site<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub author: &'a str,
    pub lang: &'a str,
    /// 站点是否存在标签页（决定是否渲染「标签」导航项）。
    pub has_tags: bool,
    /// 由 `title_bar: true` 的页面汇总而成的页眉导航项（按源文件路径排序）。
    pub nav: &'a [NavItem],
    /// 站点图标（源目录 `assets/logo.*`）的站点根相对路径；不存在时为 `None`。
    pub icon: Option<&'a str>,
}

/// 一个页眉导航项；`url` 为站点根相对路径（已做 URL 编码）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavItem {
    pub title: String,
    pub url: String,
}

/// 标签链接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagLink {
    pub name: String,
    pub url: String,
    pub count: usize,
}

/// 列表中的一篇文章。
pub struct PostLink<'a> {
    pub title: &'a str,
    pub url: String,
    pub date: Option<Date>,
    pub updated: Option<Date>,
    pub summary: &'a str,
    pub tags: Vec<TagLink>,
}

/// 文章详情页的视图数据。
pub struct PostView<'a> {
    pub title: &'a str,
    pub date: Option<Date>,
    pub updated: Option<Date>,
    pub tags: Vec<TagLink>,
    pub content: &'a str,
    pub toc: &'a [TocEntry],
}

/// 站点外壳：`<head>` + 页眉 + 主体 + 页脚。
///
/// `current` 是当前页面的站点根相对路径，用于在导航中标记 `aria-current="page"`。
fn shell(
    site: &Site<'_>,
    base: &str,
    current: Option<&str>,
    page_title: Option<&str>,
    description: &str,
    main: &str,
) -> String {
    let full_title = match page_title {
        Some(title) if title != site.title => format!("{} · {}", esc(title), esc(site.title)),
        Some(title) => esc(title),
        None => esc(site.title),
    };
    let description = if description.is_empty() {
        site.description
    } else {
        description
    };
    let meta_description = if description.is_empty() {
        String::new()
    } else {
        format!(
            "    <meta name=\"description\" content=\"{}\">\n",
            esc(description)
        )
    };
    let nav = nav_links(site, base, current);
    let icon_link = match site.icon {
        Some(icon) => {
            let href = esc(&format!("{base}{icon}"));
            match icon_mime(icon) {
                Some(mime) => format!("    <link rel=\"icon\" type=\"{mime}\" href=\"{href}\">\n"),
                None => format!("    <link rel=\"icon\" href=\"{href}\">\n"),
            }
        }
        // 源目录没有 `assets/logo.*` 时不输出任何 icon 声明。
        None => String::new(),
    };
    let footer_author = if site.author.is_empty() {
        String::new()
    } else {
        format!(
            "<span>{}</span><span class=\"dot\">·</span>",
            esc(site.author)
        )
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="{lang}">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{full_title}</title>
{meta_description}    <link rel="stylesheet" href="{base}style.css">
{icon_link}</head>
<body>
    <a class="skip-link" href="#main">跳到主内容</a>
    <header class="site-header">
        <div class="wrap header-inner">
            <a class="site-title" href="{base}index.html">{site_title}</a>
            <nav class="site-nav" aria-label="站点导航">
{nav}            </nav>
        </div>
    </header>
    <main id="main" class="wrap">
{main}    </main>
    <footer class="site-footer">
        <div class="wrap footer-inner">
            {footer_author}
        </div>
    </footer>
</body>
</html>
"##,
        lang = esc(site.lang),
        site_title = esc(site.title),
    )
}

/// 渲染页眉导航：首页 → `title_bar: true` 的页面 → 标签（若存在标签页）。
///
/// 当前所在页面会带上 `aria-current="page"` 与 `is-active` 类。
fn nav_links(site: &Site<'_>, base: &str, current: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&nav_link(
        &format!("{base}index.html"),
        "首页",
        current == Some("index.html"),
    ));
    for item in site.nav {
        out.push_str(&nav_link(
            &format!("{base}{}", item.url),
            &item.title,
            current == Some(item.url.as_str()),
        ));
    }
    if site.has_tags {
        let active =
            current.is_some_and(|path| path == "tags/index.html" || path.starts_with("tags/"));
        out.push_str(&nav_link(&format!("{base}tags/index.html"), "标签", active));
    }
    out
}

/// 由图标文件扩展名推断 MIME 类型；未知扩展名返回 `None`（只输出 `href`）。
fn icon_mime(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "ico" => Some("image/x-icon"),
        "webp" => Some("image/webp"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}

/// 生成单个导航链接；`url` 与 `label` 都会被转义。
fn nav_link(url: &str, label: &str, active: bool) -> String {
    if active {
        format!(
            "                <a class=\"is-active\" href=\"{}\" aria-current=\"page\">{}</a>\n",
            esc(url),
            esc(label)
        )
    } else {
        format!(
            "                <a href=\"{}\">{}</a>\n",
            esc(url),
            esc(label)
        )
    }
}

/// 渲染文章详情页；`current` 为该页面的站点根相对路径。
pub fn post(site: &Site<'_>, base: &str, current: &str, view: &PostView<'_>) -> String {
    let mut meta = Vec::new();
    if let Some(date) = view.date {
        meta.push(format!(
            "<time datetime=\"{}\">{}</time>",
            date.iso(),
            esc(&date.to_string())
        ));
    }
    if let Some(updated) = view.updated {
        meta.push(format!(
            "<span class=\"updated\">更新于 <time datetime=\"{}\">{}</time></span>",
            updated.iso(),
            esc(&updated.to_string())
        ));
    }
    if !view.tags.is_empty() {
        meta.push(tag_list(&view.tags));
    }
    let meta_html = if meta.is_empty() {
        String::new()
    } else {
        format!(
            "      <p class=\"post-meta\">{}</p>\n",
            meta.join("<span class=\"dot\">·</span>")
        )
    };

    let toc = toc_nav(view.toc);
    let main = format!(
        r#"    <article class="post">
      <header class="post-header">
        <h1 class="post-title">{title}</h1>
{meta_html}      </header>
{toc}      <div class="post-content">
{content}
      </div>
    </article>
"#,
        title = esc(view.title),
        content = view.content,
    );

    shell(
        site,
        base,
        Some(current),
        Some(view.title),
        &first_text(view.content),
        &main,
    )
}

/// 渲染首页：站点简介 + 可选的首页正文 + 文章列表 + 标签云。
pub fn index(
    site: &Site<'_>,
    base: &str,
    home_title: Option<&str>,
    home_html: &str,
    posts: &[PostLink<'_>],
    tags: &[TagLink],
) -> String {
    let heading = home_title.unwrap_or(site.title);
    let intro = if site.description.is_empty() {
        String::new()
    } else {
        format!(
            "        <p class=\"site-description\">{}</p>\n",
            esc(site.description)
        )
    };
    let home_body = if home_html.trim().is_empty() {
        String::new()
    } else {
        format!("        <div class=\"post-content home-content\">\n{home_html}\n        </div>\n")
    };
    let list = if posts.is_empty() {
        "      <p class=\"empty\">还没有文章，先在源目录中放一个 <code>.md</code> 文件吧。</p>\n"
            .to_string()
    } else {
        posts.iter().map(post_card).collect::<Vec<_>>().join("")
    };
    let tag_cloud = if tags.is_empty() {
        String::new()
    } else {
        format!(
            r#"      <section class="tag-section" aria-labelledby="tags-heading">
        <h2 class="section-title" id="tags-heading">标签</h2>
        <ul class="tag-cloud">
{items}        </ul>
      </section>
"#,
            items = tag_cloud_items(tags)
        )
    };

    let main = format!(
        r#"    <section class="home">
      <header class="home-header">
        <h1 class="site-heading">{heading}</h1>
{intro}{home_body}      </header>
      <section class="post-list" aria-label="文章列表">
{list}      </section>
{tag_cloud}    </section>
"#,
        heading = esc(heading),
    );

    shell(
        site,
        base,
        Some("index.html"),
        None,
        site.description,
        &main,
    )
}

/// 渲染某个标签下的文章列表页；`current` 为该标签页的站点根相对路径。
pub fn tag_page(
    site: &Site<'_>,
    base: &str,
    current: &str,
    tag: &str,
    posts: &[PostLink<'_>],
) -> String {
    let list = if posts.is_empty() {
        "      <p class=\"empty\">该标签下暂无文章。</p>\n".to_string()
    } else {
        posts.iter().map(post_card).collect::<Vec<_>>().join("")
    };
    let main = format!(
        r#"    <section class="home">
      <header class="home-header">
        <h1 class="site-heading">标签：{tag}</h1>
        <p class="site-description">共 {count} 篇文章 · <a href="{base}tags/index.html">全部标签</a></p>
      </header>
      <section class="post-list" aria-label="标签文章列表">
{list}      </section>
    </section>
"#,
        tag = esc(tag),
        count = posts.len(),
    );

    shell(
        site,
        base,
        Some(current),
        Some(&format!("标签：{tag}")),
        "",
        &main,
    )
}

/// 渲染标签总览页。
pub fn tags_page(site: &Site<'_>, base: &str, tags: &[TagLink]) -> String {
    let list = if tags.is_empty() {
        "      <p class=\"empty\">还没有任何标签。</p>\n".to_string()
    } else {
        tag_cloud_items(tags)
    };
    let main = format!(
        r#"    <section class="home">
      <header class="home-header">
        <h1 class="site-heading">全部标签</h1>
        <p class="site-description">共 {count} 个标签</p>
      </header>
      <ul class="tag-cloud">
{list}      </ul>
    </section>
"#,
        count = tags.len(),
    );

    shell(
        site,
        base,
        Some("tags/index.html"),
        Some("全部标签"),
        "",
        &main,
    )
}

fn post_card(post: &PostLink<'_>) -> String {
    let mut meta = Vec::new();
    if let Some(date) = post.date {
        meta.push(format!(
            "<time datetime=\"{}\">{}</time>",
            date.iso(),
            esc(&date.to_string())
        ));
    }
    if let Some(updated) = post.updated {
        meta.push(format!(
            "<span class=\"updated\">更新于 <time datetime=\"{}\">{}</time></span>",
            updated.iso(),
            esc(&updated.to_string())
        ));
    }
    if !post.tags.is_empty() {
        meta.push(tag_list(&post.tags));
    }
    let meta_html = if meta.is_empty() {
        String::new()
    } else {
        format!(
            "          <p class=\"post-meta\">{}</p>\n",
            meta.join("<span class=\"dot\">·</span>")
        )
    };
    let summary = if post.summary.is_empty() {
        String::new()
    } else {
        format!(
            "          <p class=\"post-summary\">{}</p>\n",
            esc(post.summary)
        )
    };

    format!(
        r#"        <article class="post-card">
          <h2 class="post-card-title"><a href="{url}">{title}</a></h2>
{meta_html}{summary}        </article>
"#,
        url = esc(&post.url),
        title = esc(post.title),
    )
}

fn tag_list(tags: &[TagLink]) -> String {
    let items = tags
        .iter()
        .map(|tag| {
            format!(
                "<a class=\"tag\" href=\"{}\">{}</a>",
                esc(&tag.url),
                esc(&tag.name)
            )
        })
        .collect::<String>();
    format!("<span class=\"tag-list\">{items}</span>")
}

/// 标签云内容：每行一个 `<li>`，保持与 `<ul>` 一致的缩进。
fn tag_cloud_items(tags: &[TagLink]) -> String {
    tags.iter()
        .map(|tag| {
            format!(
                "          <li><a class=\"tag\" href=\"{}\">{}<span class=\"tag-count\">{}</span></a></li>\n",
                esc(&tag.url),
                esc(&tag.name),
                tag.count
            )
        })
        .collect()
}

fn toc_nav(toc: &[TocEntry]) -> String {
    if toc.len() < 2 {
        return String::new();
    }
    let items = toc
        .iter()
        .map(|entry| {
            format!(
                "          <li class=\"toc-item toc-level-{level}\"><a href=\"#{id}\">{text}</a></li>\n",
                level = entry.level,
                id = esc(&entry.id),
                text = esc(&entry.text),
            )
        })
        .collect::<String>();
    format!(
        r##"      <nav class="toc" aria-labelledby="toc-heading">
        <h2 class="toc-heading" id="toc-heading">目录</h2>
        <ul class="toc-list">
{items}        </ul>
      </nav>
"##
    )
}

/// 从正文 HTML 中取一段纯文本用于 `<meta name="description">`。
fn first_text(html: &str) -> String {
    crate::markdown::first_paragraph(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定的一组导航项，供测试复用。
    fn nav() -> Vec<NavItem> {
        vec![
            NavItem {
                title: "关于本站".to_string(),
                url: "about.html".to_string(),
            },
            NavItem {
                title: "订阅".to_string(),
                url: "feed/index.html".to_string(),
            },
        ]
    }

    fn site() -> Site<'static> {
        Site {
            title: "示例站点",
            description: "描述",
            author: "作者",
            lang: "zh-CN",
            has_tags: true,
            nav: &[],
            icon: None,
        }
    }

    fn tag(name: &str) -> TagLink {
        TagLink {
            name: name.to_string(),
            url: format!("tags/{name}.html"),
            count: 1,
        }
    }

    /// 一个最小的文章视图，供外壳相关断言复用。
    fn view_fixture() -> PostView<'static> {
        PostView {
            title: "标题",
            date: None,
            updated: None,
            tags: Vec::new(),
            content: "<p>正文</p>",
            toc: &[],
        }
    }

    #[test]
    fn post_page_contains_meta_and_toc() {
        let toc = vec![
            TocEntry {
                level: 2,
                id: "a".into(),
                text: "A".into(),
            },
            TocEntry {
                level: 2,
                id: "b".into(),
                text: "B".into(),
            },
        ];
        let view = PostView {
            title: "标题",
            date: Some(Date::parse("2025-01-02").unwrap()),
            updated: None,
            tags: vec![tag("rust")],
            content: "<p>正文</p>",
            toc: &toc,
        };
        let html = post(&site(), "", "hello.html", &view);
        assert!(html.contains("<title>标题 · 示例站点</title>"));
        assert!(html.contains("datetime=\"2025-01-02\""));
        assert!(html.contains("href=\"tags/rust.html\""));
        assert!(html.contains("class=\"toc\""));
        assert!(html.contains("href=\"#a\""));
    }

    #[test]
    fn index_page_escapes_user_text() {
        let posts = vec![PostLink {
            title: "<script>",
            url: "a.html".into(),
            date: None,
            updated: None,
            summary: "s",
            tags: Vec::new(),
        }];
        let html = index(&site(), "", None, "", &posts, &[tag("rust")]);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("class=\"tag-cloud\""));
    }

    #[test]
    fn relative_prefix_is_used_in_links() {
        let html = tags_page(
            &site(),
            "../",
            &[TagLink {
                name: "rust".into(),
                url: "tags/rust.html".into(),
                count: 2,
            }],
        );
        assert!(html.contains("href=\"../style.css\""));
        assert!(html.contains("href=\"../index.html\""));
        assert!(html.contains("href=\"tags/rust.html\""));
    }

    #[test]
    fn nav_renders_title_bar_pages_between_home_and_tags() {
        let nav = nav();
        let site = Site {
            nav: &nav,
            ..site()
        };
        let html = nav_links(&site, "../", Some("about.html"));

        assert!(html.contains("<a href=\"../index.html\">首页</a>"));
        assert!(html.contains(
            "<a class=\"is-active\" href=\"../about.html\" aria-current=\"page\">关于本站</a>"
        ));
        assert!(html.contains("<a href=\"../feed/index.html\">订阅</a>"));
        assert!(html.contains("<a href=\"../tags/index.html\">标签</a>"));

        // 顺序固定为：首页 → title_bar 页面 → 标签。
        let home = html.find("首页").unwrap();
        let about = html.find("关于本站").unwrap();
        let tags = html.find(">标签<").unwrap();
        assert!(home < about && about < tags);
    }

    #[test]
    fn nav_marks_current_page() {
        let nav = nav();
        let site = Site {
            nav: &nav,
            ..site()
        };

        let home = nav_links(&site, "", Some("index.html"));
        assert!(
            home.contains(
                "<a class=\"is-active\" href=\"index.html\" aria-current=\"page\">首页</a>"
            )
        );

        // 标签子页也应把「标签」标记为当前项。
        let tag = nav_links(&site, "../", Some("tags/rust.html"));
        assert!(tag.contains(
            "<a class=\"is-active\" href=\"../tags/index.html\" aria-current=\"page\">标签</a>"
        ));
        assert!(
            !tag.contains("关于本站</a>") || !tag.contains("is-active\" href=\"../about.html\"")
        );
    }

    /// 站点外壳的固定组成部分，避免模板改动时静默丢失。
    #[test]
    fn shell_contains_landmarks() {
        let html = post(&site(), "", "hello.html", &view_fixture());
        for landmark in [
            "<meta charset=\"utf-8\">",
            "class=\"skip-link\" href=\"#main\"",
            "id=\"main\"",
            "class=\"site-footer\"",
            "<span>使用 <strong>asog</strong> 生成</span>",
            "<span>作者</span>",
        ] {
            assert!(html.contains(landmark), "缺少 {landmark}：{html}");
        }
        // 没有配置图标时不输出任何 icon 声明。
        assert!(!html.contains("rel=\"icon\""));
    }

    #[test]
    fn icon_is_rendered_with_type_and_prefix_when_present() {
        let svg = Site {
            icon: Some("assets/logo.svg"),
            ..site()
        };
        let html = post(&svg, "../", "about.html", &view_fixture());
        assert!(
            html.contains("<link rel=\"icon\" type=\"image/svg+xml\" href=\"../assets/logo.svg\">")
        );

        let png = Site {
            icon: Some("assets/logo.png"),
            ..site()
        };
        let html = post(&png, "", "about.html", &view_fixture());
        assert!(html.contains("<link rel=\"icon\" type=\"image/png\" href=\"assets/logo.png\">"));
    }

    #[test]
    fn icon_skips_unknown_extensions_without_type() {
        let bmp = Site {
            icon: Some("assets/logo.bmp"),
            ..site()
        };
        let html = post(&bmp, "", "about.html", &view_fixture());
        assert!(html.contains("<link rel=\"icon\" href=\"assets/logo.bmp\">"));
        assert!(!html.contains("type=\""));
    }

    #[test]
    fn icon_mime_mapping() {
        assert_eq!(icon_mime("assets/logo.svg"), Some("image/svg+xml"));
        assert_eq!(icon_mime("a/b/LOGO.PNG"), Some("image/png"));
        assert_eq!(icon_mime("logo.ico"), Some("image/x-icon"));
        assert_eq!(icon_mime("logo.jpeg"), Some("image/jpeg"));
        assert_eq!(icon_mime("logo"), None);
        assert_eq!(icon_mime("logo.bmp"), None);
    }

    #[test]
    fn nav_escapes_titles_and_is_absent_without_items() {
        let nav = vec![NavItem {
            title: "<b>粗体</b>".to_string(),
            url: "a.html".to_string(),
        }];
        let with_nav = Site {
            nav: &nav,
            ..site()
        };
        let html = nav_links(&with_nav, "", None);
        assert!(html.contains("&lt;b&gt;粗体&lt;/b&gt;"));
        assert!(!html.contains("<b>粗体</b>"));

        let plain = nav_links(&site(), "", None);
        assert_eq!(
            plain,
            concat!(
                "                <a href=\"index.html\">首页</a>\n",
                "                <a href=\"tags/index.html\">标签</a>\n"
            )
        );
    }
}

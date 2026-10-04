//! Markdown 编译：调用 `markdown` crate 把源文本编译为 HTML，
//! 并为标题补充锚点 id 与目录信息。

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::util::{normalize_whitespace, slugify, strip_tags, truncate_chars};

/// 编译选项。
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    /// 是否允许 Markdown 中的原始 HTML（本地站点内容默认视为可信）。
    pub raw_html: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { raw_html: true }
    }
}

/// 一条目录项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub text: String,
}

/// 编译结果。
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocEntry>,
}

/// 把 Markdown 编译为 HTML，并返回标题目录。
pub fn render(source: &str, options: &RenderOptions) -> Result<Rendered> {
    let mut markdown_options = markdown::Options::gfm();
    markdown_options.compile.allow_dangerous_html = options.raw_html;
    if options.raw_html {
        // GFM 的 tagfilter 会过滤掉部分“危险”标签，允许原始 HTML 时一并关闭。
        markdown_options.compile.gfm_tagfilter = false;
    }

    let html = markdown::to_html_with_options(source, &markdown_options)
        .map_err(|message| anyhow!("Markdown 编译失败: {message}"))?;

    let (html, toc) = add_heading_anchors(&html);
    Ok(Rendered { html, toc })
}

/// 为 HTML 中的所有 `<h1>`–`<h6>` 添加唯一 `id` 与可点击的锚点链接，
/// 同时按出现顺序返回目录信息。
pub fn add_heading_anchors(html: &str) -> (String, Vec<TocEntry>) {
    let mut out = String::with_capacity(html.len() + 64);
    let mut toc = Vec::new();
    let mut used: HashMap<String, usize> = HashMap::new();
    let mut cursor = 0usize;

    loop {
        let Some(relative) = html[cursor..].find('<') else {
            out.push_str(&html[cursor..]);
            break;
        };
        let lt = cursor + relative;
        out.push_str(&html[cursor..lt]);

        let Some((level, open_end)) = heading_open_at(html, lt) else {
            out.push('<');
            cursor = lt + 1;
            continue;
        };
        let close = format!("</h{level}>");
        let Some(close_relative) = html[open_end..].find(&close) else {
            // 没有闭合标签：按普通文本处理，避免破坏原文。
            out.push('<');
            cursor = lt + 1;
            continue;
        };
        let close_start = open_end + close_relative;
        let inner = &html[open_end..close_start];
        let text = normalize_whitespace(&strip_tags(inner));
        let id = unique_id(&text, &mut used);

        // `open_end` 指向 `>` 之后，因此 `html[lt..open_end - 1]` 正是 `<h2`。
        out.push_str(&html[lt..open_end - 1]);
        out.push_str(&format!(" id=\"{id}\">"));
        out.push_str(inner);
        out.push_str(&format!(
            "<a class=\"heading-anchor\" href=\"#{id}\" aria-hidden=\"true\" tabindex=\"-1\">#</a>"
        ));
        toc.push(TocEntry { level, id, text });
        cursor = close_start;
    }

    (out, toc)
}

/// 判断 `position` 处是否为形如 `<h2>` 的标题起始标签。
fn heading_open_at(html: &str, position: usize) -> Option<(u8, usize)> {
    let bytes = html.as_bytes();
    if bytes.get(position) != Some(&b'<') || bytes.get(position + 1) != Some(&b'h') {
        return None;
    }
    let level = *bytes.get(position + 2)?;
    if !(b'1'..=b'6').contains(&level) || bytes.get(position + 3) != Some(&b'>') {
        return None;
    }
    Some((level - b'0', position + 4))
}

fn unique_id(text: &str, used: &mut HashMap<String, usize>) -> String {
    let base = {
        let slug = slugify(text);
        if slug.is_empty() {
            "section".to_string()
        } else {
            slug
        }
    };
    let counter = used.entry(base.clone()).or_insert(0);
    let id = if *counter == 0 {
        base.clone()
    } else {
        format!("{base}-{counter}")
    };
    *counter += 1;
    id
}

/// 取正文中第一个一级标题（缺省时退化为二级标题）作为标题。
pub fn first_heading(source: &str) -> Option<String> {
    let mut h1 = None;
    let mut h2 = None;
    let mut fence: Option<&str> = None;

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        let Some(text) = trimmed.strip_prefix("# ") else {
            if h2.is_none() {
                h2 = trimmed.strip_prefix("## ").map(clean_inline);
            }
            continue;
        };
        let text = clean_inline(text);
        if text.is_empty() {
            continue;
        }
        if h1.is_none() {
            h1 = Some(text);
        }
    }

    h1.or(h2)
}

/// 若正文以一级标题开头，则去掉该行（用于标题已被用作文章标题的场景）。
pub fn strip_first_heading(source: &str) -> &str {
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            offset += line.len();
            continue;
        }
        if trimmed.starts_with("# ") {
            return source[offset + line.len()..].trim_start_matches(['\n', '\r']);
        }
        return source;
    }
    source
}

fn clean_inline(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches('#').trim();
    let stripped: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '`' | '*'))
        .collect();
    normalize_whitespace(&stripped)
}

/// 取渲染后 HTML 的第一个非空段落作为摘要。
pub fn first_paragraph(html: &str) -> String {
    let mut cursor = 0usize;
    while let Some(relative) = html[cursor..].find("<p>") {
        let start = cursor + relative + 3;
        let Some(end_relative) = html[start..].find("</p>") else {
            break;
        };
        let end = start + end_relative;
        let text = normalize_whitespace(&strip_tags(&html[start..end]));
        if !text.is_empty() {
            return truncate_chars(&text, 200);
        }
        cursor = end;
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_gfm_markdown() {
        let rendered = render(
            "# 标题\n\n- [x] 任务\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(rendered.html.contains("<table>"));
        assert!(rendered.html.contains("type=\"checkbox\""));
        assert!(rendered.html.contains("<h1 id=\"标题\">"));
    }

    #[test]
    fn adds_unique_heading_ids_and_toc() {
        let rendered = render("# A\n\n## B\n\n## B\n", &RenderOptions::default()).unwrap();
        assert!(rendered.html.contains("<h1 id=\"a\">"));
        assert!(rendered.html.contains("<h2 id=\"b\">"));
        assert!(rendered.html.contains("<h2 id=\"b-1\">"));
        assert_eq!(rendered.toc.len(), 3);
        assert_eq!(rendered.toc[1].level, 2);
        assert_eq!(rendered.toc[1].text, "B");
        assert!(rendered.html.contains("class=\"heading-anchor\""));
        // `id` 必须落在起始标签内部，不能泄漏成正文文本。
        assert!(
            !rendered.html.contains("> id="),
            "生成的 HTML 有误：{}",
            rendered.html
        );
    }

    #[test]
    fn heading_anchor_links_match_the_generated_ids() {
        let rendered = render("## 中文 标题\n", &RenderOptions::default()).unwrap();
        assert!(rendered.html.contains("<h2 id=\"中文-标题\">"));
        assert!(rendered.html.contains("href=\"#中文-标题\""));
        assert!(rendered.html.contains("</a></h2>"));
    }

    #[test]
    fn strips_inline_markup_when_building_ids() {
        let rendered = render("## `code` 与 **粗体**\n", &RenderOptions::default()).unwrap();
        assert!(rendered.html.contains("<h2 id=\"code-与-粗体\">"));
        assert_eq!(rendered.toc[0].text, "code 与 粗体");
    }

    #[test]
    fn leaves_code_blocks_untouched() {
        let rendered = render(
            "```html\n<h2>不是标题</h2>\n```\n",
            &RenderOptions::default(),
        )
        .unwrap();
        assert!(!rendered.html.contains("heading-anchor"));
        assert!(rendered.toc.is_empty());
        assert!(rendered.html.contains("&lt;h2&gt;"));
    }

    #[test]
    fn raw_html_is_opt_in() {
        let with_html = render(
            "<div class=\"x\">hi</div>\n",
            &RenderOptions { raw_html: true },
        )
        .unwrap();
        assert!(with_html.html.contains("<div class=\"x\">"));

        let without_html = render(
            "<div class=\"x\">hi</div>\n",
            &RenderOptions { raw_html: false },
        )
        .unwrap();
        assert!(!without_html.html.contains("<div class=\"x\">"));
    }

    #[test]
    fn finds_and_strips_leading_heading() {
        let source = "\n# 我的标题\n\n正文段落\n";
        assert_eq!(first_heading(source).as_deref(), Some("我的标题"));
        assert_eq!(strip_first_heading(source).trim(), "正文段落");

        let code = "```\n# 注释\n```\n\n## 真标题\n";
        assert_eq!(first_heading(code).as_deref(), Some("真标题"));
        assert_eq!(strip_first_heading(code), code);
    }

    #[test]
    fn derives_summary_from_first_paragraph() {
        let rendered = render("第一段文字。\n\n第二段。\n", &RenderOptions::default()).unwrap();
        assert_eq!(first_paragraph(&rendered.html), "第一段文字。");
        assert_eq!(first_paragraph("<img src=\"x\">"), "");
    }
}

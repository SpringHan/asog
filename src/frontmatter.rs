//! Markdown 文件的前置元数据（front matter）解析。
//!
//! 支持在文件开头使用 `---` 围栏的键值块，例如：
//!
//! ```text
//! ---
//! title: 你好，世界
//! date: 2025-01-02 10:30
//! tags: [rust, 博客]
//! summary: 一篇文章的摘要
//! draft: false
//! toc: true
//! ---
//!
//! # 正文标题
//! ```
//!
//! 解析器只实现博客场景够用的 YAML 子集：`key: value`、行内列表
//! `[a, b]`、逗号分隔列表以及缩进块列表；无法识别的行会被记录为警告
//! 而不是直接报错，从而保证坏元数据不会中断整站构建。

use std::fmt;

/// 解析后的前置元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    /// 文章标题；缺省时由一级标题或文件名推导。
    pub title: Option<String>,
    /// 发布 / 创建时间。
    pub date: Option<Date>,
    /// 最后更新时间。
    pub updated: Option<Date>,
    /// 标签（`tags` / `categories` 会合并到这里）。
    pub tags: Vec<String>,
    /// 摘要；缺省时取正文首段。
    pub summary: Option<String>,
    /// 是否为草稿，草稿默认不参与构建。
    pub draft: bool,
    /// 是否在首页 / 标签页的列表中展示。
    pub listed: bool,
    /// 是否在文章页生成目录。
    pub toc: bool,
    /// 是否把该页面加入页眉导航（默认为 `false`）。
    pub title_bar: bool,
    /// 自定义文件名 slug（当前用于文章页元信息，保留给后续扩展）。
    pub slug: Option<String>,
    /// 解析过程中的非致命问题。
    pub warnings: Vec<String>,
}

impl Default for FrontMatter {
    fn default() -> Self {
        Self {
            title: None,
            date: None,
            updated: None,
            tags: Vec::new(),
            summary: None,
            draft: false,
            listed: true,
            toc: false,
            title_bar: false,
            slug: None,
            warnings: Vec::new(),
        }
    }
}

/// 仅含年月日与可选时间的简易日期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Date {
    /// 解析 `YYYY-MM-DD`、`YYYY/MM/DD`、`YYYY-MM-DD HH:MM(:SS)`（可带时区后缀）。
    pub fn parse(input: &str) -> Option<Self> {
        let text = input.trim().trim_matches(['"', '\'']).trim();
        if text.is_empty() {
            return None;
        }
        let (date_part, rest) = match text.find(['T', ' ']) {
            Some(index) => (&text[..index], text[index + 1..].trim()),
            None => (text, ""),
        };
        let mut parts = date_part.split(['-', '/', '.']);
        let year: i32 = parts.next()?.trim().parse().ok()?;
        let month: u32 = parts.next()?.trim().parse().ok()?;
        let day: u32 = parts.next()?.trim().parse().ok()?;
        if year < 1 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
            return None;
        }

        let (mut hour, mut minute, mut second) = (0, 0, 0);
        if !rest.is_empty() {
            let time: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ':')
                .collect();
            let mut parts = time.split(':');
            hour = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            minute = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            second = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
            if hour > 23 || minute > 59 || second > 59 {
                return None;
            }
        }

        Some(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
        })
    }

    /// 是否包含具体时刻（用于决定展示与 `datetime` 属性的精度）。
    pub fn has_time(&self) -> bool {
        self.hour != 0 || self.minute != 0 || self.second != 0
    }

    /// 供 `<time datetime="...">` 使用的 ISO 8601 形式。
    pub fn iso(&self) -> String {
        if self.has_time() {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute, self.second
            )
        } else {
            format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
        }
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_time() {
            write!(
                f,
                "{:04}-{:02}-{:02} {:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute
            )
        } else {
            write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
        }
    }
}

/// 若源文件以 `---` 围栏开头，返回（围栏内容, 正文）。
///
/// 围栏未闭合时返回 `None`，此时整个文件都按正文处理。
pub fn split(source: &str) -> Option<(&str, &str)> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut lines = source.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim() != "---" {
        return None;
    }

    let block_start = first.len();
    let mut cursor = block_start;
    for line in lines {
        let next = cursor + line.len();
        let marker = line.trim_end_matches(['\r', '\n']).trim_end();
        if marker == "---" || marker == "..." {
            let block = &source[block_start..cursor];
            let body = source[next..].trim_start_matches(['\n', '\r']);
            return Some((block, body));
        }
        cursor = next;
    }
    None
}

/// 解析整个 Markdown 源文件，返回前置元数据与正文切片。
pub fn parse(source: &str) -> (FrontMatter, &str) {
    let Some((block, body)) = split(source) else {
        return (FrontMatter::default(), source);
    };
    let mut front = FrontMatter::default();
    parse_block(block, &mut front);
    (front, body)
}

fn parse_block(block: &str, front: &mut FrontMatter) {
    let mut list_key: Option<String> = None;
    for line in block.lines() {
        let line = line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indented = line.starts_with(' ') || line.starts_with('\t');
        if indented {
            match (trimmed.strip_prefix("- "), list_key.as_deref()) {
                (Some(item), Some(key)) => apply_list_item(front, key, unquote(item.trim())),
                (Some(_), None) => {
                    front
                        .warnings
                        .push(format!("忽略无法归属的列表项: {trimmed}"));
                }
                (None, _) => front
                    .warnings
                    .push(format!("忽略无法解析的续行: {trimmed}")),
            }
            continue;
        }

        let Some((key, value)) = line.split_once(':') else {
            front.warnings.push(format!("忽略无法解析的行: {trimmed}"));
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        if value.is_empty() {
            // 可能是块列表的开头，例如 `tags:` 后跟缩进的 `- rust`。
            if key == "tags" || key == "tag" || key == "categories" || key == "category" {
                front.tags.clear();
            }
            list_key = Some(key);
            continue;
        }

        list_key = None;
        apply_scalar(front, &key, value);
    }
}

fn apply_scalar(front: &mut FrontMatter, key: &str, value: &str) {
    match key {
        "title" => front.title = Some(unquote(value)),
        "date" | "published" | "pubdate" | "created" => match Date::parse(value) {
            Some(date) => front.date = Some(date),
            None => front.warnings.push(format!("无法解析日期 `{value}`")),
        },
        "updated" | "lastmod" | "modified" => match Date::parse(value) {
            Some(date) => front.updated = Some(date),
            None => front.warnings.push(format!("无法解析更新日期 `{value}`")),
        },
        "tags" | "tag" | "categories" | "category" => front.tags.extend(parse_list(value)),
        "summary" | "description" | "excerpt" | "desc" => front.summary = Some(unquote(value)),
        "draft" | "unpublished" => match parse_bool(value) {
            Some(flag) => front.draft = flag,
            None => front.warnings.push(format!("无法解析布尔值 `{value}`")),
        },
        "listed" | "visible" => match parse_bool(value) {
            Some(flag) => front.listed = flag,
            None => front.warnings.push(format!("无法解析布尔值 `{value}`")),
        },
        "index" => match parse_bool(value) {
            // `index: false` 表示不进入列表（Hugo 风格）。
            Some(flag) => front.listed = flag,
            None => front.warnings.push(format!("无法解析布尔值 `{value}`")),
        },
        "toc" | "toc_depth" => match parse_bool(value) {
            Some(flag) => front.toc = flag,
            None => front.warnings.push(format!("无法解析布尔值 `{value}`")),
        },
        "title_bar" | "title-bar" | "titlebar" => match parse_bool(value) {
            Some(flag) => front.title_bar = flag,
            None => front.warnings.push(format!(
                "无法解析布尔值 `{value}`（title_bar 只能为 true 或 false）"
            )),
        },
        "slug" | "permalink" => front.slug = Some(unquote(value)),
        other => front
            .warnings
            .push(format!("未知的前置元数据字段 `{other}`")),
    }
}

fn apply_list_item(front: &mut FrontMatter, key: &str, item: String) {
    match key {
        "tags" | "tag" | "categories" | "category" => {
            if !item.is_empty() {
                front.tags.push(item);
            }
        }
        other => front
            .warnings
            .push(format!("字段 `{other}` 不支持列表值，已忽略")),
    }
}

/// 解析行内列表 `[a, b]` 或逗号分隔列表 `a, b`。
pub fn parse_list(value: &str) -> Vec<String> {
    let value = value.trim();
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or(value);
    inner
        .split(',')
        .map(|item| unquote(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_bool(value: &str) -> Option<bool> {
    // 与其它字段保持一致：允许 `true` / `"true"` / `'yes'` 等写法。
    match unquote(value).trim().to_ascii_lowercase().as_str() {
        "true" | "yes" | "on" | "1" => Some(true),
        "false" | "no" | "off" | "0" => Some(false),
        _ => None,
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[quote.len_utf8()..value.len() - quote.len_utf8()].to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalar_fields() {
        let source = "---\ntitle: 你好\ndate: 2025-01-02 10:30\ndraft: true\nlisted: false\ntoc: yes\n---\n\n# 正文\n";
        let (front, body) = parse(source);
        assert_eq!(front.title.as_deref(), Some("你好"));
        assert_eq!(front.date, Date::parse("2025-01-02 10:30"));
        assert!(front.draft);
        assert!(!front.listed);
        assert!(front.toc);
        assert_eq!(body.trim(), "# 正文");
        assert!(front.warnings.is_empty());
    }

    #[test]
    fn title_bar_defaults_to_false_and_only_accepts_booleans() {
        // 默认不进入页眉导航。
        let (front, _) = parse("---\nlisted: false\n---\nbody");
        assert!(!front.title_bar);

        let (front, _) = parse("---\ntitle_bar: true\n---\nbody");
        assert!(front.title_bar);

        // 兼容中划线写法与常见布尔字面量。
        let (front, _) = parse("---\ntitle-bar: 'yes'\n---\nbody");
        assert!(front.title_bar);
        let (front, _) = parse("---\ntitle_bar: false\n---\nbody");
        assert!(!front.title_bar);

        // 非法值：保持 false 并给出警告。
        let (front, _) = parse("---\ntitle_bar: maybe\n---\nbody");
        assert!(!front.title_bar);
        assert_eq!(front.warnings.len(), 1);
        assert!(front.warnings[0].contains("title_bar"));
    }

    #[test]
    fn parses_inline_and_block_lists() {
        let (front, _) = parse("---\ntags: [rust, 博客]\n---\nbody");
        assert_eq!(front.tags, vec!["rust", "博客"]);

        let (front, _) = parse("---\ntags:\n  - rust\n  - '静态站点'\n---\nbody");
        assert_eq!(front.tags, vec!["rust", "静态站点"]);

        let (front, _) = parse("---\ncategories: rust, blog\n---\nbody");
        assert_eq!(front.tags, vec!["rust", "blog"]);
    }

    #[test]
    fn keeps_defaults_when_front_matter_missing() {
        let (front, body) = parse("# 标题\n\n正文");
        assert_eq!(front, FrontMatter::default());
        assert!(front.listed);
        assert_eq!(body, "# 标题\n\n正文");
    }

    #[test]
    fn unterminated_front_matter_is_body() {
        let source = "---\ntitle: 未闭合\n\n正文";
        let (front, body) = parse(source);
        assert!(front.title.is_none());
        assert_eq!(body, source);
    }

    #[test]
    fn records_warnings_for_bad_lines() {
        let (front, _) = parse("---\njust a line\ndate: not-a-date\nunknown: 1\n---\n");
        assert_eq!(front.warnings.len(), 3);
        assert!(front.date.is_none());
    }

    #[test]
    fn date_ordering_and_rendering() {
        let older = Date::parse("2024-12-31").unwrap();
        let newer = Date::parse("2025-01-02T08:05:00Z").unwrap();
        assert!(older < newer);
        assert_eq!(older.to_string(), "2024-12-31");
        assert_eq!(newer.to_string(), "2025-01-02 08:05");
        assert_eq!(newer.iso(), "2025-01-02T08:05:00");
        assert!(Date::parse("2025-13-01").is_none());
        assert!(Date::parse("hello").is_none());
    }
}

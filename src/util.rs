//! 通用工具：HTML 转义、slug、文本清理与路径处理。

use std::path::{Component, Path, PathBuf};

/// 转义 HTML 文本 / 属性值中的特殊字符。
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// 把任意文本转换为可安全用作文件名 / URL 片段的 slug。
///
/// 规则：仅保留字母与数字（Unicode），其余字符折叠为 `-`，并转为小写。
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut pending_dash = false;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = !out.is_empty();
        }
    }
    out
}

/// 去除 HTML 标签并解码常见实体，得到纯文本。
pub fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '<' => {
                for c in chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                }
            }
            '&' => decode_entity(&mut chars, &mut out),
            _ => out.push(ch),
        }
    }
    out
}

fn decode_entity(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    let mut entity = String::new();
    let mut closed = false;
    while let Some(&c) = chars.peek() {
        if c == ';' {
            chars.next();
            closed = true;
            break;
        }
        if c.is_ascii_alphanumeric() || c == '#' {
            if entity.len() >= 8 {
                break;
            }
            entity.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if !closed {
        out.push('&');
        out.push_str(&entity);
        return;
    }
    match entity.as_str() {
        "amp" => out.push('&'),
        "lt" => out.push('<'),
        "gt" => out.push('>'),
        "quot" => out.push('"'),
        "apos" => out.push('\''),
        "nbsp" => out.push(' '),
        _ => match entity.strip_prefix('#').and_then(parse_numeric_entity) {
            Some(ch) => out.push(ch),
            None => {
                out.push('&');
                out.push_str(&entity);
                out.push(';');
            }
        },
    }
}

fn parse_numeric_entity(value: &str) -> Option<char> {
    let code = match value.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => value.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

/// 合并空白、去除首尾空白后的纯文本。
pub fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 按字符数截断文本，超出部分以省略号结尾。
pub fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(input.len().min(max_chars * 4));
    for (count, ch) in input.chars().enumerate() {
        if count == max_chars {
            out.push('…');
            return out;
        }
        out.push(ch);
    }
    out
}

/// 百分号编码 URL 路径，保留 `/` 分隔符。
pub fn encode_uri_path(path: &str) -> String {
    const KEEP: &[u8] = b"-._~/";
    let mut out = String::with_capacity(path.len());
    for byte in path.as_bytes() {
        let c = *byte;
        if c.is_ascii_alphanumeric() || KEEP.contains(&c) {
            out.push(c as char);
        } else {
            out.push('%');
            out.push_str(&format!("{c:02X}"));
        }
    }
    out
}

/// 把相对路径转换为使用 `/` 分隔、可直接用于 `href` 的 URL。
pub fn rel_url(path: &Path) -> String {
    let raw = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    encode_uri_path(&raw)
}

/// 由输出文件的相对路径推导出回到站点根目录所需的相对前缀（`../` 的重复）。
pub fn relative_prefix(rel_out: &Path) -> String {
    let depth = match rel_out.parent() {
        Some(parent) => parent
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .count(),
        None => 0,
    };
    "../".repeat(depth)
}

/// 判断相对路径中是否含有以 `.` 开头的隐藏目录或文件。
pub fn is_hidden(rel: &Path) -> bool {
    rel.components().any(|c| match c {
        Component::Normal(name) => name.to_string_lossy().starts_with('.'),
        _ => false,
    })
}

/// 把相对路径转换为绝对路径（不做文件系统访问）。
pub fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// 在不访问文件系统的前提下做词法规范化：消除 `.` 与多余的 `..`。
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                _ => out.push(".."),
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 把文件路径整理为站点内使用的绝对路径 URL（以 `/` 开头）。
pub fn site_url(rel: &Path) -> String {
    format!("/{}", rel_url(rel))
}

/// 由文件名推导默认标题：去掉扩展名，把 `-`/`_` 替换为空格并把首字母大写。
pub fn title_from_filename(rel: &Path) -> String {
    let stem = rel
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "未命名".to_string());
    let spaced = stem.replace(['-', '_'], " ");
    let trimmed = spaced.trim();
    if trimmed.is_empty() {
        return "未命名".to_string();
    }
    let mut chars = trimmed.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => "未命名".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html_specials() {
        assert_eq!(
            escape_html(r#"<a href="x">&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
    }

    #[test]
    fn slugify_folds_separators_and_lowercases() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  Rust 2024 Edition  "), "rust-2024-edition");
        assert_eq!(slugify("中文 标签"), "中文-标签");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn strip_tags_decodes_entities() {
        assert_eq!(strip_tags("<p>a &amp; b</p>"), "a & b");
        assert_eq!(strip_tags("<code>&lt;div&gt;</code>"), "<div>");
        assert_eq!(strip_tags("1 &lt; 2 &#38; 3 &#x26; 4"), "1 < 2 & 3 & 4");
        assert_eq!(strip_tags("unknown &foo; here"), "unknown &foo; here");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate_chars("abcdef", 3), "abc…");
        assert_eq!(truncate_chars("中文测试", 2), "中文…");
        assert_eq!(truncate_chars("abc", 5), "abc");
    }

    #[test]
    fn encodes_uri_paths() {
        assert_eq!(encode_uri_path("a b/c.md"), "a%20b/c.md");
        assert_eq!(
            encode_uri_path("中文/index.html"),
            "%E4%B8%AD%E6%96%87/index.html"
        );
    }

    #[test]
    fn computes_relative_prefix() {
        assert_eq!(relative_prefix(Path::new("index.html")), "");
        assert_eq!(relative_prefix(Path::new("tags/rust.html")), "../");
        assert_eq!(relative_prefix(Path::new("a/b/c.html")), "../../");
    }

    #[test]
    fn normalizes_paths_lexically() {
        assert_eq!(
            normalize_path(Path::new("/a/./b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_path(Path::new("a/../../b")),
            PathBuf::from("../b")
        );
    }

    #[test]
    fn detects_hidden_components() {
        assert!(is_hidden(Path::new(".git/config")));
        assert!(!is_hidden(Path::new("posts/index.md")));
    }
}

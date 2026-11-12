//! Clean HTML-to-text and Readability-style article body extraction.
//!
//! Used in two places:
//! 1. RSS/Atom feed excerpts and `<content:encoded>` HTML fragments (`clean_feed_html`).
//! 2. Full web page extraction (`extract_web_article`) so the internal Reader and Hub
//!    analysts have clean paragraph text without scripts, menus, ads or cookie walls.

pub const MAX_SUMMARY_CHARS: usize = 420;
pub const MAX_CONTENT_CHARS: usize = 120_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedArticle {
    pub summary: Option<String>,
    pub content_text: String,
    pub word_count: usize,
}

/// Counts whitespace-delimited words in clean text.
pub fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Builds a concise plain-text summary truncated cleanly at a word boundary.
pub fn make_excerpt(text: &str, max_chars: usize) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= max_chars {
        return Some(collapsed);
    }
    let mut out = String::new();
    for word in collapsed.split_whitespace() {
        let next_len = if out.is_empty() {
            word.chars().count()
        } else {
            out.chars().count() + 1 + word.chars().count()
        };
        if next_len + 1 > max_chars {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = collapsed.chars().take(max_chars.saturating_sub(1)).collect();
    }
    out.push('…');
    Some(out)
}

/// Converts an HTML fragment (such as RSS `<description>` or `<content:encoded>`) into
/// clean, paragraph-structured plain text with entities decoded.
pub fn html_to_clean_text(raw: &str) -> String {
    let without_comments = strip_comments(raw);
    let cleaned = strip_tag_blocks(
        &without_comments,
        &[
            "script", "style", "noscript", "svg", "iframe", "canvas", "form", "button",
            "select", "nav", "footer", "aside",
        ],
    );
    let mut out = String::with_capacity(cleaned.len().min(MAX_CONTENT_CHARS));
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(rel_end) = cleaned[i..].find('>')
        {
            let tag_inner = cleaned[i + 1..i + rel_end].trim();
            let is_closing = tag_inner.starts_with('/');
            let name_slice = tag_inner.trim_start_matches('/');
            let tag_name: String = name_slice
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .map(|c| c.to_ascii_lowercase())
                .collect();
            match tag_name.as_str() {
                "br" | "hr" if !is_closing => out.push('\n'),
                "li" if !is_closing => out.push_str("\n• "),
                "li" if is_closing => out.push('\n'),
                "h1" | "h2" | "h3" | "h4" if !is_closing => out.push_str("\n\n"),
                "h1" | "h2" | "h3" | "h4" if is_closing => out.push_str("\n\n"),
                "p" | "div" | "section" | "article" | "blockquote" | "pre" | "tr"
                | "table" | "ul" | "ol" => out.push_str("\n\n"),
                "td" | "th" if is_closing => out.push_str("  "),
                _ => {}
            }
            i += rel_end + 1;
            continue;
        }
        let ch = cleaned[i..].chars().next().unwrap_or(' ');
        if ch.is_control() && ch != '\n' && ch != '\t' {
            out.push(' ');
        } else {
            out.push(ch);
        }
        i += ch.len_utf8();
    }
    let decoded = decode_entities(&out);
    normalize_paragraphs(&decoded)
}

/// Extracts a clean summary + optional full body from RSS/Atom fields.
pub fn extract_from_feed(
    raw_description: Option<&str>,
    raw_content: Option<&str>,
    article_title: &str,
) -> (Option<String>, Option<String>, usize) {
    let desc_clean = raw_description
        .map(html_to_clean_text)
        .filter(|s| !s.is_empty() && !same_as_title(s, article_title) && !is_download_boilerplate(s));
    let content_clean = raw_content
        .map(html_to_clean_text)
        .filter(|s| !s.is_empty() && !same_as_title(s, article_title) && !is_download_boilerplate(s));

    let best_body = match (&content_clean, &desc_clean) {
        (Some(c), Some(d)) => {
            if c.chars().count() >= d.chars().count() {
                Some(c.clone())
            } else {
                Some(d.clone())
            }
        }
        (Some(c), None) => Some(c.clone()),
        (None, Some(d)) if d.chars().count() >= 280 => Some(d.clone()),
        _ => None,
    };

    let summary = desc_clean
        .as_deref()
        .and_then(|d| make_excerpt(d, MAX_SUMMARY_CHARS))
        .or_else(|| {
            best_body
                .as_deref()
                .and_then(|b| make_excerpt(b, MAX_SUMMARY_CHARS))
        });

    if let Some(body) = best_body {
        let truncated: String = body.chars().take(MAX_CONTENT_CHARS).collect();
        let wc = count_words(&truncated);
        if wc >= 40 {
            return (summary, Some(truncated), wc);
        }
    }
    (summary, None, 0)
}

/// Extracts the main readable article text from a full HTML (or plain-text) response.
pub fn extract_web_article(bytes: &[u8], title_hint: &str) -> Option<ExtractedArticle> {
    if bytes.is_empty() {
        return None;
    }
    // Reject obvious binary payloads (PDF, ZIP, PNG, JPEG).
    if bytes.starts_with(b"%PDF-")
        || bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(b"\xFF\xD8\xFF")
    {
        return None;
    }
    let raw = String::from_utf8_lossy(bytes);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Plain-text response (no HTML tags at all).
    let lower_head = trimmed.chars().take(1024).collect::<String>().to_ascii_lowercase();
    if !lower_head.contains("<html")
        && !lower_head.contains("<body")
        && !lower_head.contains("<article")
        && !lower_head.contains("<div")
        && !lower_head.contains("<p")
    {
        let normalized = normalize_paragraphs(&decode_entities(trimmed));
        let wc = count_words(&normalized);
        if wc < 20 {
            return None;
        }
        let body: String = normalized.chars().take(MAX_CONTENT_CHARS).collect();
        let summary = make_excerpt(&body, MAX_SUMMARY_CHARS);
        return Some(ExtractedArticle {
            summary,
            word_count: count_words(&body),
            content_text: body,
        });
    }

    let meta_desc = extract_meta_description(&raw)
        .map(|d| html_to_clean_text(&d))
        .filter(|d| !d.is_empty() && !same_as_title(d, title_hint));

    let no_comments = strip_comments(&raw);
    let stripped = strip_tag_blocks(
        &no_comments,
        &[
            "head", "script", "style", "noscript", "svg", "iframe", "canvas", "form",
            "button", "select", "nav", "footer", "aside", "header",
        ],
    );
    let stripped = strip_noisy_containers(&stripped);

    let mut candidates: Vec<(usize, String)> = Vec::new();
    for tag in ["article", "main"] {
        for inner in collect_tag_contents(&stripped, tag) {
            let text = filter_boilerplate_lines(&html_to_clean_text(&inner));
            let score = score_candidate(&text, tag == "article");
            if score > 0 {
                candidates.push((score, text));
            }
        }
    }
    for inner in collect_content_divs(&stripped) {
        let text = filter_boilerplate_lines(&html_to_clean_text(&inner));
        let score = score_candidate(&text, true);
        if score > 0 {
            candidates.push((score, text));
        }
    }

    let chosen = if let Some((_, best)) = candidates.into_iter().max_by_key(|(s, _)| *s) {
        best
    } else {
        let body_html = collect_tag_contents(&stripped, "body")
            .into_iter()
            .next()
            .unwrap_or(stripped);
        filter_boilerplate_lines(&html_to_clean_text(&body_html))
    };

    let final_body = if count_words(&chosen) >= 25 {
        chosen
    } else if let Some(ref desc) = meta_desc
        && count_words(desc) >= 15
    {
        desc.clone()
    } else {
        return None;
    };

    let content_text: String = final_body.chars().take(MAX_CONTENT_CHARS).collect();
    let word_count = count_words(&content_text);
    let summary = meta_desc
        .as_deref()
        .and_then(|d| make_excerpt(d, MAX_SUMMARY_CHARS))
        .or_else(|| make_excerpt(&content_text, MAX_SUMMARY_CHARS));

    Some(ExtractedArticle {
        summary,
        content_text,
        word_count,
    })
}

fn same_as_title(text: &str, title: &str) -> bool {
    let a = text.trim().to_lowercase();
    let b = title.trim().to_lowercase();
    !b.is_empty() && a == b
}

fn is_download_boilerplate(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with("content files") && count_words(&lower) <= 16
}

fn score_candidate(text: &str, preferred: bool) -> usize {
    let words = count_words(text);
    if words < 25 {
        return 0;
    }
    let paragraphs = text.split("\n\n").filter(|p| p.split_whitespace().count() >= 8).count();
    let base = words + paragraphs * 35;
    if preferred { base + 120 } else { base }
}

fn filter_boilerplate_lines(text: &str) -> String {
    let mut kept = Vec::new();
    for para in text.split("\n\n") {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        let lower = p.to_lowercase();
        if p.chars().count() < 90
            && (lower.contains("all rights reserved")
                || lower.contains("accept all cookies")
                || lower.contains("manage cookie")
                || lower.contains("skip to main content")
                || lower.contains("subscribe to our newsletter")
                || lower.contains("đăng ký nhận bản tin"))
        {
            continue;
        }
        kept.push(p);
    }
    kept.join("\n\n")
}

fn strip_comments(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start + 4..].find("-->") {
            rest = &rest[start + 4 + end + 3..];
        } else {
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn strip_tag_blocks(html: &str, tags: &[&str]) -> String {
    let mut current = html.to_string();
    for tag in tags {
        current = remove_single_tag_block(&current, tag);
    }
    current
}

fn remove_single_tag_block(html: &str, tag: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(&open_pat) {
        let start = cursor + rel;
        let after_name = start + open_pat.len();
        let valid_boundary = lower
            .as_bytes()
            .get(after_name)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid_boundary {
            out.push_str(&html[cursor..after_name]);
            cursor = after_name;
            continue;
        }
        out.push_str(&html[cursor..start]);
        if let Some(close_rel) = lower[after_name..].find(&close_pat) {
            let close_start = after_name + close_rel;
            if let Some(gt) = lower[close_start..].find('>') {
                cursor = close_start + gt + 1;
                continue;
            }
        }
        if let Some(gt) = lower[after_name..].find('>') {
            cursor = after_name + gt + 1;
        } else {
            return out;
        }
    }
    out.push_str(&html[cursor..]);
    out
}

/// Removes `<div>`/`<section>` blocks whose class/id is purely navigation, comments or ads.
fn strip_noisy_containers(html: &str) -> String {
    const NOISE: &[&str] = &[
        "sidebar", "comment", "related", "cookie", "newsletter", "breadcrumb", "share-bar",
        "social-share", "advert", "ad-slot", "footer", "navbar", "masthead",
    ];
    const KEEP: &[&str] = &[
        "article", "content", "story", "post", "entry", "detail", "fck_detail", "main",
    ];
    let mut out = html.to_string();
    for tag in ["div", "section"] {
        out = filter_containers_by_attr(&out, tag, |attrs| {
            let a = attrs.to_ascii_lowercase();
            let has_noise = NOISE.iter().any(|k| a.contains(k));
            let has_keep = KEEP.iter().any(|k| a.contains(k));
            !(has_noise && !has_keep)
        });
    }
    out
}

fn filter_containers_by_attr<F>(html: &str, tag: &str, keep_fn: F) -> String
where
    F: Fn(&str) -> bool,
{
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}");
    let mut out = String::with_capacity(html.len());
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(&open_pat) {
        let start = cursor + rel;
        let after = start + open_pat.len();
        let valid = lower
            .as_bytes()
            .get(after)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid {
            out.push_str(&html[cursor..after]);
            cursor = after;
            continue;
        }
        let Some(gt_rel) = lower[after..].find('>') else {
            break;
        };
        let attrs = &html[after..after + gt_rel];
        if keep_fn(attrs) {
            out.push_str(&html[cursor..after + gt_rel + 1]);
            cursor = after + gt_rel + 1;
            continue;
        }
        out.push_str(&html[cursor..start]);
        if let Some(end) = find_matching_close(&lower, after + gt_rel + 1, &open_pat, &close_pat) {
            cursor = end;
        } else {
            cursor = after + gt_rel + 1;
        }
    }
    out.push_str(&html[cursor..]);
    out
}

fn collect_tag_contents(html: &str, tag: &str) -> Vec<String> {
    let lower = html.to_ascii_lowercase();
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}");
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(&open_pat) {
        let start = cursor + rel;
        let after = start + open_pat.len();
        let valid = lower
            .as_bytes()
            .get(after)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid {
            cursor = after;
            continue;
        }
        let Some(gt_rel) = lower[after..].find('>') else {
            break;
        };
        let inner_start = after + gt_rel + 1;
        if let Some(end) = find_matching_close(&lower, inner_start, &open_pat, &close_pat) {
            let close_start = lower[inner_start..end]
                .rfind(&close_pat)
                .map(|idx| inner_start + idx)
                .unwrap_or(end);
            results.push(html[inner_start..close_start].to_string());
            cursor = end;
        } else {
            cursor = inner_start;
        }
    }
    results
}

fn collect_content_divs(html: &str) -> Vec<String> {
    const HINTS: &[&str] = &[
        "fck_detail",
        "article-body",
        "article_body",
        "article-content",
        "post-content",
        "entry-content",
        "story-body",
        "detail-content",
        "main-content",
        "usa-prose",
    ];
    let lower = html.to_ascii_lowercase();
    let open_pat = "<div";
    let close_pat = "</div";
    let mut results = Vec::new();
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find(open_pat) {
        let start = cursor + rel;
        let after = start + open_pat.len();
        let valid = lower
            .as_bytes()
            .get(after)
            .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        if !valid {
            cursor = after;
            continue;
        }
        let Some(gt_rel) = lower[after..].find('>') else {
            break;
        };
        let attrs = &lower[after..after + gt_rel];
        let inner_start = after + gt_rel + 1;
        if HINTS.iter().any(|h| attrs.contains(h))
            && let Some(end) = find_matching_close(&lower, inner_start, open_pat, close_pat)
        {
            let close_start = lower[inner_start..end]
                .rfind(close_pat)
                .map(|idx| inner_start + idx)
                .unwrap_or(end);
            results.push(html[inner_start..close_start].to_string());
            cursor = end;
            continue;
        }
        cursor = inner_start;
    }
    results
}

fn find_matching_close(lower: &str, from: usize, open_pat: &str, close_pat: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut pos = from;
    while pos < lower.len() {
        let next_open = lower[pos..].find(open_pat).map(|i| pos + i);
        let next_close = lower[pos..].find(close_pat).map(|i| pos + i)?;
        if let Some(open_idx) = next_open
            && open_idx < next_close
        {
            let after = open_idx + open_pat.len();
            let valid = lower
                .as_bytes()
                .get(after)
                .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
            if valid
                && let Some(gt) = lower[after..].find('>')
            {
                let self_closing = lower[after..after + gt].trim_end().ends_with('/');
                if !self_closing {
                    depth += 1;
                }
                pos = after + gt + 1;
                continue;
            }
            pos = after;
            continue;
        }
        depth -= 1;
        let gt = lower[next_close..].find('>')?;
        pos = next_close + gt + 1;
        if depth == 0 {
            return Some(pos);
        }
    }
    None
}

fn extract_meta_description(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut cursor = 0;
    while let Some(rel) = lower[cursor..].find("<meta") {
        let start = cursor + rel;
        let Some(gt_rel) = lower[start..].find('>') else {
            break;
        };
        let tag = &html[start..start + gt_rel + 1];
        let tag_lower = &lower[start..start + gt_rel + 1];
        cursor = start + gt_rel + 1;
        if (tag_lower.contains("og:description")
            || tag_lower.contains("name=\"description\"")
            || tag_lower.contains("name='description'")
            || tag_lower.contains("twitter:description"))
            && let Some(content) = extract_attr_value(tag, "content")
        {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn extract_attr_value(tag: &str, attr_name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let pat = format!("{attr_name}=");
    let idx = lower.find(&pat)?;
    let rest = tag[idx + pat.len()..].trim_start();
    let quote = rest.chars().next()?;
    if quote == '"' || quote == '\'' {
        let inner = &rest[quote.len_utf8()..];
        let end = inner.find(quote)?;
        Some(inner[..end].to_string())
    } else {
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>')
            .unwrap_or(rest.len());
        Some(rest[..end].to_string())
    }
}

pub fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        if let Some(semi) = after.bytes().take(12).position(|b| b == b';') {
            let entity = &after[..semi];
            if let Some(decoded) = resolve_entity(entity) {
                out.push_str(&decoded);
                rest = &after[semi + 1..];
                continue;
            }
        }
        out.push('&');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn resolve_entity(entity: &str) -> Option<String> {
    if let Some(hex) = entity.strip_prefix("#x").or_else(|| entity.strip_prefix("#X")) {
        let code = u32::from_str_radix(hex, 16).ok()?;
        return char::from_u32(code).map(|c| c.to_string());
    }
    if let Some(dec) = entity.strip_prefix('#') {
        let code = dec.parse::<u32>().ok()?;
        return char::from_u32(code).map(|c| c.to_string());
    }
    let mapped = match entity {
        "nbsp" => " ",
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" | "#39" => "'",
        "mdash" => "—",
        "ndash" => "–",
        "rsquo" | "lsquo" => "'",
        "rdquo" | "ldquo" => "\"",
        "hellip" => "…",
        "bull" => "•",
        "euro" => "€",
        "pound" => "£",
        "yen" => "¥",
        "copy" => "©",
        _ => return None,
    };
    Some(mapped.to_string())
}

fn normalize_paragraphs(raw: &str) -> String {
    let mut paragraphs = Vec::new();
    let mut current_lines: Vec<String> = Vec::new();
    for line in raw.lines() {
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            if !current_lines.is_empty() {
                paragraphs.push(current_lines.join("\n"));
                current_lines.clear();
            }
        } else {
            current_lines.push(collapsed);
        }
    }
    if !current_lines.is_empty() {
        paragraphs.push(current_lines.join("\n"));
    }
    paragraphs.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_fragment_cleans_tags_and_decodes_vietnamese_entities() {
        let raw = "<p>Ng&#226;n h&#224;ng Nh&#224; n&#432;&#7899;c gi&#7919; l&#227;i su&#7845;t &mdash; &#x111;&#7891;ng VND &#7893;n &#273;&#7883;nh.</p><script>alert(1)</script><ul><li>Mục 1</li><li>Mục 2</li></ul>";
        let clean = html_to_clean_text(raw);
        assert!(clean.contains("Ngân hàng Nhà nước giữ lãi suất — đồng VND ổn định."));
        assert!(clean.contains("• Mục 1"));
        assert!(!clean.contains("alert(1)"));
    }

    #[test]
    fn web_article_extractor_picks_article_and_drops_nav_footer_and_sidebar() {
        let html = r#"<!doctype html>
        <html>
          <head>
            <meta property="og:description" content="Tóm tắt nhanh quyết định lãi suất của Fed trong kỳ họp tháng 11." />
          </head>
          <body>
            <nav><a href="/">Trang chủ</a> <a href="/kinh-doanh">Kinh doanh</a></nav>
            <div class="sidebar-related">Bài liên quan cần bỏ qua để không nhiễu nội dung đọc.</div>
            <article class="fck_detail">
              <h1>Fed giữ nguyên lãi suất điều hành ở mức hiện tại</h1>
              <p>Cục Dự trữ Liên bang Mỹ (Fed) vừa công bố quyết định giữ nguyên biên độ lãi suất mục tiêu sau hai ngày họp chính sách tiền tệ tại Washington.</p>
              <p>Chủ tịch Jerome Powell nhấn mạnh lạm phát đã hạ nhiệt đáng kể so với đỉnh năm ngoái nhưng cơ quan này vẫn cần thêm dữ liệu thị trường lao động trước khi nới lỏng thêm.</p>
              <p>Lợi suất trái phiếu kho bạc kỳ hạn 10 năm giảm nhẹ 6 điểm cơ bản ngay sau thông cáo báo chí chính thức.</p>
            </article>
            <footer>All rights reserved 2026</footer>
          </body>
        </html>"#
        .as_bytes();

        let extracted = extract_web_article(html, "Fed giữ nguyên lãi suất").expect("extracted");
        assert!(extracted.summary.as_deref().unwrap().contains("Tóm tắt nhanh quyết định lãi suất"));
        assert!(extracted.content_text.contains("Cục Dự trữ Liên bang Mỹ (Fed)"));
        assert!(extracted.content_text.contains("Jerome Powell"));
        assert!(!extracted.content_text.contains("Bài liên quan cần bỏ qua"));
        assert!(!extracted.content_text.contains("All rights reserved"));
        assert!(extracted.word_count >= 50);
    }

    #[test]
    fn binary_or_empty_js_shell_returns_none() {
        assert!(extract_web_article(b"%PDF-1.7 binary payload", "Doc").is_none());
        let shell = b"<html><body><div id='app'></div><script src='bundle.js'></script></body></html>";
        assert!(extract_web_article(shell, "SPA").is_none());
    }
}

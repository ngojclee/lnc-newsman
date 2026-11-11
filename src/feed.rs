//! RSS 2.0 / RSS 1.0 (RDF) / Atom parser that keeps metadata only.
//!
//! Descriptions and article bodies are deliberately ignored: Newsman stores the
//! title, link, identifier and publication time, never the publisher's text.

use chrono::{DateTime, NaiveDateTime, Utc};
use quick_xml::{Reader, events::Event};
use url::Url;

pub const MAX_ITEMS: usize = 200;
const MAX_TITLE_CHARS: usize = 300;
const MAX_GUID_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq)]
pub struct FeedItem {
    pub title: String,
    /// Canonical http(s) article URL.
    pub url: String,
    pub guid: Option<String>,
    /// RFC 3339 UTC, e.g. `2026-11-11T08:00:00Z`.
    pub published_at: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum FeedError {
    NotAFeed,
    Malformed,
}

impl FeedError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotAFeed => "not_a_feed",
            Self::Malformed => "invalid_feed",
        }
    }
}

#[derive(Default)]
struct Draft {
    title: String,
    link: Option<String>,
    alternate_link: Option<String>,
    guid: Option<String>,
    guid_is_link: bool,
    published: Option<String>,
    updated: Option<String>,
}

/// Parse a feed body. `base` resolves relative links. `now` rejects dates far in the future.
///
/// Two tolerances are deliberate and tested: mismatched end tags are ignored
/// (`check_end_names = false`) and a broken tail after at least one usable item is
/// kept. The feed root element (`rss`, `feed` or `RDF`) is still required, so an HTML
/// page that happens to contain `<item>` markup is not accepted as a feed.
pub fn parse(bytes: &[u8], base: &Url, now: DateTime<Utc>) -> Result<Vec<FeedItem>, FeedError> {
    let mut reader = Reader::from_reader(bytes);
    let config = reader.config_mut();
    config.trim_text(false);
    config.check_end_names = false;

    let mut buf = Vec::new();
    let mut depth = 0usize;
    let mut saw_feed_root = false;
    let mut item_depth: Option<usize> = None;
    let mut draft = Draft::default();
    let mut field: Option<(String, usize)> = None;
    let mut text = String::new();
    let mut items = Vec::new();

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event,
            // Keep what was parsed before a broken tail; fail only when nothing was usable.
            Err(_) if !items.is_empty() && saw_feed_root => break,
            Err(_) => return Err(FeedError::Malformed),
        };
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let is_empty = matches!(event, Event::Empty(_));
                let name = local(start.local_name().as_ref());
                if depth == 0 && matches!(name.as_str(), "rss" | "feed" | "RDF") {
                    saw_feed_root = true;
                }
                if item_depth.is_none() && matches!(name.as_str(), "item" | "entry") {
                    if !is_empty {
                        item_depth = Some(depth);
                        draft = Draft::default();
                    }
                } else if let Some(item) = item_depth
                    && depth == item + 1
                {
                    if name == "link"
                        && let Some(href) = attr(start, b"href")
                        && matches!(attr(start, b"rel").as_deref(), None | Some("alternate"))
                    {
                        draft.alternate_link.get_or_insert(href);
                    }
                    if name == "guid" {
                        draft.guid_is_link =
                            attr(start, b"isPermaLink").as_deref() != Some("false");
                    }
                    if !is_empty {
                        field = Some((name, depth));
                        text.clear();
                    }
                }
                if !is_empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if let Some((name, field_depth)) = &field
                    && *field_depth == depth
                {
                    assign(&mut draft, name, text.trim());
                    field = None;
                    text.clear();
                }
                if item_depth == Some(depth) {
                    item_depth = None;
                    if let Some(item) = finish(std::mem::take(&mut draft), base, now) {
                        items.push(item);
                        if items.len() >= MAX_ITEMS {
                            break;
                        }
                    }
                }
            }
            Event::Text(t) if field.is_some() => {
                if let Ok(value) = t.decode() {
                    push_text(&mut text, &value);
                }
            }
            Event::CData(c) if field.is_some() => {
                if let Ok(value) = c.decode() {
                    push_text(&mut text, &value);
                }
            }
            Event::GeneralRef(r) if field.is_some() => {
                if let Ok(Some(ch)) = r.resolve_char_ref() {
                    text.push(ch);
                } else if let Ok(name) = r.decode() {
                    text.push_str(match name.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => " ",
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    if !saw_feed_root {
        // Items without a feed root come from an HTML page or arbitrary XML, not a feed.
        return Err(FeedError::NotAFeed);
    }
    Ok(items)
}

fn local(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).into_owned()
}

fn attr(start: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    start
        .attributes()
        .flatten()
        .find(|a| a.key.local_name().as_ref() == name)
        .and_then(|a| a.unescape_value().ok().map(|v| v.trim().to_string()))
        .filter(|v| !v.is_empty())
}

fn push_text(text: &mut String, value: &str) {
    if text.len() < 64 * 1024 {
        text.push_str(value);
    }
}

fn assign(draft: &mut Draft, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    match name {
        "title" if draft.title.is_empty() => draft.title = value.to_string(),
        "link" if draft.link.is_none() => draft.link = Some(value.to_string()),
        "guid" | "id" if draft.guid.is_none() => draft.guid = Some(value.to_string()),
        "pubDate" | "published" | "issued" | "date" if draft.published.is_none() => {
            draft.published = Some(value.to_string())
        }
        "updated" | "modified" if draft.updated.is_none() => {
            draft.updated = Some(value.to_string())
        }
        _ => {}
    }
}

fn finish(draft: Draft, base: &Url, now: DateTime<Utc>) -> Option<FeedItem> {
    let title = clean_title(&draft.title)?;
    let candidates = [
        draft.alternate_link.as_deref(),
        draft.link.as_deref(),
        draft.guid.as_deref().filter(|_| draft.guid_is_link),
    ];
    let url = candidates
        .into_iter()
        .flatten()
        .find_map(|link| canonical_url(link, base))?;
    let guid = draft
        .guid
        .map(|g| g.chars().take(MAX_GUID_CHARS).collect::<String>());
    let published_at = draft
        .published
        .or(draft.updated)
        .and_then(|value| parse_date(&value, now));
    Some(FeedItem {
        title,
        url,
        guid,
        published_at,
    })
}

/// Titles may carry escaped HTML; strip tags, decode common entities and collapse whitespace.
pub fn clean_title(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for ch in raw.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(if ch.is_control() { ' ' } else { ch }),
            _ => {}
        }
    }
    let decoded = out
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");
    let collapsed = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(MAX_TITLE_CHARS).collect())
}

const TRACKING_PARAMS: &[&str] = &[
    "fbclid", "gclid", "dclid", "msclkid", "mc_cid", "mc_eid", "igshid", "_ga",
];

/// Resolve against the feed URL, accept only http(s) without credentials,
/// drop the fragment and common tracking parameters.
pub fn canonical_url(link: &str, base: &Url) -> Option<String> {
    let mut url = base.join(link.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    url.host_str()?;
    url.set_fragment(None);
    if url.query().is_some() {
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(k, _)| {
                let key = k.to_ascii_lowercase();
                !key.starts_with("utm_") && !TRACKING_PARAMS.contains(&key.as_str())
            })
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if kept.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(kept);
        }
    }
    let value = url.to_string();
    (value.len() <= 2048).then_some(value)
}

pub fn parse_date(value: &str, now: DateTime<Utc>) -> Option<String> {
    let value = value.trim();
    let parsed = DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S"]
                .iter()
                .find_map(|fmt| NaiveDateTime::parse_from_str(value, fmt).ok())
                .map(|naive| naive.and_utc())
        })?;
    if parsed > now + chrono::Duration::days(1) {
        return None;
    }
    Some(parsed.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-11-11T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn base() -> Url {
        Url::parse("https://news.example.org/feeds/main.xml").unwrap()
    }

    #[test]
    fn parses_rss_without_keeping_descriptions() {
        let xml = br#"<?xml version="1.0"?><rss version="2.0"><channel><title>Feed</title><link>https://news.example.org/</link>
            <item><title>Ng&#226;n h&#224;ng &amp; l&#227;i su&#7845;t</title><link>/a/1?utm_source=x&amp;id=7#top</link>
              <guid isPermaLink="false">abc-1</guid><pubDate>Tue, 10 Nov 2026 08:30:00 +0100</pubDate>
              <description>&lt;p&gt;Full publisher text that must not be stored&lt;/p&gt;</description></item>
            <item><title><![CDATA[<b>Second</b>   story]]></title><guid>https://news.example.org/a/2</guid></item>
            <item><title>No link</title></item>
            <item><title>Bad scheme</title><link>javascript:alert(1)</link></item>
          </channel></rss>"#;
        let items = parse(xml, &base(), now()).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Ngân hàng & lãi suất");
        assert_eq!(items[0].url, "https://news.example.org/a/1?id=7");
        assert_eq!(items[0].guid.as_deref(), Some("abc-1"));
        assert_eq!(
            items[0].published_at.as_deref(),
            Some("2026-11-10T07:30:00Z")
        );
        assert_eq!(items[1].title, "Second story");
        assert_eq!(items[1].url, "https://news.example.org/a/2");
        assert!(format!("{items:?}").find("publisher text").is_none());
    }

    #[test]
    fn parses_atom_and_rdf() {
        let atom = br#"<feed xmlns="http://www.w3.org/2005/Atom"><title>A</title>
            <entry><title type="html">Atom &lt;i&gt;one&lt;/i&gt;</title><id>tag:x,2026:1</id>
              <link rel="self" href="https://news.example.org/self"/><link rel="alternate" href="https://news.example.org/atom/1"/>
              <updated>2026-11-11T09:00:00Z</updated></entry></feed>"#;
        let items = parse(atom, &base(), now()).unwrap();
        assert_eq!(items[0].title, "Atom one");
        assert_eq!(items[0].url, "https://news.example.org/atom/1");
        assert_eq!(
            items[0].published_at.as_deref(),
            Some("2026-11-11T09:00:00Z")
        );

        let rdf = br#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns="http://purl.org/rss/1.0/" xmlns:dc="http://purl.org/dc/elements/1.1/">
            <item rdf:about="https://news.example.org/r/1"><title>RDF item</title><link>https://news.example.org/r/1</link><dc:date>2026-11-11T10:00:00+07:00</dc:date></item></rdf:RDF>"#;
        let items = parse(rdf, &base(), now()).unwrap();
        assert_eq!(items[0].url, "https://news.example.org/r/1");
        assert_eq!(
            items[0].published_at.as_deref(),
            Some("2026-11-11T03:00:00Z")
        );
    }

    #[test]
    fn keeps_what_parsed_before_a_broken_tail() {
        let xml = br#"<rss version="2.0"><channel><title>F</title>
            <item><title>Good one</title><link>https://news.example.org/good</link></item>
            <item><title>Truncated"#;
        let items = parse(xml, &base(), now()).unwrap();
        assert_eq!(items.len(), 1, "a usable item survives a truncated tail");
        assert_eq!(items[0].title, "Good one");
    }

    #[test]
    fn tolerates_mismatched_end_tags() {
        let xml = br#"<rss version="2.0"><channel><item><title>Mismatched</title><link>https://news.example.org/m</link></item></chanel></rss>"#;
        let items = parse(xml, &base(), now()).unwrap();
        assert_eq!(items[0].title, "Mismatched");
    }

    #[test]
    fn html_with_item_markup_is_not_a_feed() {
        // Previously accepted: `<item>` elements alone, without a feed root.
        let html = br#"<html><body><ul><item><title>Fake</title><link>https://news.example.org/x</link></item></ul></body></html>"#;
        assert_eq!(parse(html, &base(), now()), Err(FeedError::NotAFeed));
    }

    #[test]
    fn rejects_html_pages_and_future_dates() {
        assert_eq!(
            parse(b"<html><body>Hello</body></html>", &base(), now()),
            Err(FeedError::NotAFeed)
        );
        assert_eq!(parse_date("Fri, 01 Jan 2100 00:00:00 GMT", now()), None);
        assert_eq!(
            canonical_url("https://user:pw@news.example.org/x", &base()),
            None
        );
        assert_eq!(canonical_url("ftp://news.example.org/x", &base()), None);
    }
}

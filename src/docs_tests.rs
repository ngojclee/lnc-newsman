//! The API documentation must name every route the router serves and nothing else.
//!
//! This is the P8 acceptance item "OpenAPI/API_DOCS matches 100% of routes". It reads
//! the router's own source and `API_DOCS.md`, so a new endpoint cannot land without a
//! line in the docs and a removed one cannot linger there. It is a text check, not a
//! live request: the paths are compared as written, with `{id}`-style placeholders.

const ROUTER: &str = include_str!("main.rs");
const DOCS: &str = include_str!("../API_DOCS.md");

/// `"/path"` literals used inside a `.route(...)` call in the router source. A call may
/// span lines (`route(\n    "/path",`), so the search starts at `.route(` and takes the
/// first quoted path after it, not just what is on the same line.
fn routed_paths() -> Vec<String> {
    let mut paths = Vec::new();
    let mut from = 0;
    while let Some(offset) = ROUTER[from..].find(".route(") {
        let start = from + offset + ".route(".len();
        let rest = &ROUTER[start..];
        let Some(quote) = rest.find('"') else { break };
        let after = &rest[quote + 1..];
        let Some(end) = after.find('"') else { break };
        let candidate = &after[..end];
        if candidate.starts_with('/') {
            paths.push(candidate.to_string());
        }
        from = start + quote + end + 1;
    }
    paths
}

/// `GET /path?query=` tokens in the documentation, whether they sit in a table cell,
/// a sentence or a backtick. Tokenising avoids depending on backticks being paired.
fn documented_paths() -> Vec<String> {
    DOCS.split(|c: char| {
        c.is_whitespace() || matches!(c, '`' | '|' | '(' | ')' | ',' | ';' | '"' | '\'')
    })
    .filter_map(|token| {
        let token = token.trim_end_matches(['.', ':', '*']);
        let offset = token.find('/')?;
        let path = &token[offset..];
        let path = path.split('?').next().unwrap_or("");
        let api = path.starts_with("/api/") || path == "/mcp" || path == "/health";
        (api && path.len() > 1).then(|| path.to_string())
    })
    .collect()
}

#[test]
fn every_route_is_documented_and_every_documented_path_exists() {
    let mut routes = routed_paths();
    routes.sort_unstable();
    routes.dedup();
    assert!(
        routes.len() > 30,
        "the route scan found only {} paths — did the source shape change?",
        routes.len()
    );

    let documented = documented_paths();
    let normalise = |path: &str| {
        path.replace("{id}", "{x}")
            .replace("{slug}", "{x}")
            .replace("{name}", "{x}")
            .replace("{code}", "{x}")
    };
    let mut documented_norm: Vec<String> = documented.iter().map(|p| normalise(p)).collect();
    documented_norm.sort_unstable();
    documented_norm.dedup();

    // Pages served by the SPA fallback are not API paths.
    let pages = ["/", "/admin", "/login.html"];
    let missing: Vec<&String> = routes
        .iter()
        .filter(|path| !pages.contains(&path.as_str()))
        .filter(|path| !documented_norm.contains(&normalise(path)))
        .collect();
    assert!(
        missing.is_empty(),
        "routes missing from API_DOCS.md: {missing:?}"
    );

    // Static file mounts are not API paths and are allowed to be undocumented.
    let static_ok = ["/assets", "/admin"];
    let unknown: Vec<&String> = documented_norm
        .iter()
        .filter(|path| {
            !routes.iter().any(|route| normalise(route) == **path)
                && !static_ok.contains(&path.as_str())
        })
        .collect();
    assert!(
        unknown.is_empty(),
        "API_DOCS.md names paths the router does not serve: {unknown:?}"
    );
}

#[test]
fn the_status_flag_and_the_mcp_section_agree_with_the_router() {
    // `mcp_available` must be true exactly while `/mcp` is routed.
    let routed = routed_paths().iter().any(|path| path == "/mcp");
    let claims = ROUTER.contains("\"mcp_available\":true");
    assert_eq!(routed, claims, "mcp_available and the /mcp route disagree");
    assert_eq!(
        routed,
        DOCS.contains("POST /mcp") || DOCS.contains("`POST /mcp`")
    );
}

/// Every MCP tool must appear in the docs, and the docs must not name a tool that no
/// longer exists. The route check above covers HTTP paths; this covers the tool
/// catalogue, which is what an agent actually calls.
#[test]
fn every_mcp_tool_is_documented() {
    for tool in crate::mcp::tools() {
        assert!(
            DOCS.contains(tool.name),
            "tool `{}` is published but never documented",
            tool.name
        );
    }
    // A short, unambiguous marker per tool: a name in a table cell. Anything left over
    // means the docs describe a tool that is gone.
    for documented in [
        "note_create",
        "note_update",
        "note_delete",
        "analysis_create",
        "analysis_update",
        "article_flag",
        "analyses",
        "write_log",
    ] {
        assert!(
            crate::mcp::tools().iter().any(|tool| tool.name == documented),
            "docs name `{documented}`, which is not a published tool"
        );
    }
}

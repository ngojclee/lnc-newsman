//! Region tree and region packs (P1.5).
//!
//! A region pack is a directory tree. The pack root **is** the virtual region
//! `global`; every first-level directory is a child of it. A directory becomes a
//! region when it holds `region.toml`, and directory nesting is the parent/child
//! relation.
//!
//! Detection is not activation: scanning (at startup and on "Reload regions") only
//! registers regions and sources, always disabled, except the always-on `global`
//! root. Nothing is fetched until an admin enables a region and a source, and the
//! scheduler only looks at enabled regions plus their ancestors.

use crate::{SharedState, catalog, db, failure, internal_error, now};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::Path as FsPath,
};

const MAX_DEPTH: usize = 8;
pub const ROOT_CODE: &str = "global";
const MAX_ISSUES: usize = 200;

/// Pack files embedded in the binary; written to the region directory on first run
/// so the owner can edit them without rebuilding. Existing files are never touched.
const BUILTIN: &[(&str, &str)] = &[
    ("region.toml", include_str!("../assets/regions/region.toml")),
    (
        "sources.toml",
        include_str!("../assets/regions/sources.toml"),
    ),
    (
        "eu/region.toml",
        include_str!("../assets/regions/eu/region.toml"),
    ),
    (
        "eu/sources.toml",
        include_str!("../assets/regions/eu/sources.toml"),
    ),
    (
        "eu/fr/region.toml",
        include_str!("../assets/regions/eu/fr/region.toml"),
    ),
    (
        "eu/fr/sources.toml",
        include_str!("../assets/regions/eu/fr/sources.toml"),
    ),
    (
        "us/region.toml",
        include_str!("../assets/regions/us/region.toml"),
    ),
    (
        "us/sources.toml",
        include_str!("../assets/regions/us/sources.toml"),
    ),
    (
        "vn/region.toml",
        include_str!("../assets/regions/vn/region.toml"),
    ),
    (
        "vn/sources.toml",
        include_str!("../assets/regions/vn/sources.toml"),
    ),
    (
        "official.toml",
        include_str!("../assets/regions/official.toml"),
    ),
    ("tax.toml", include_str!("../assets/regions/tax.toml")),
    (
        "eu/official.toml",
        include_str!("../assets/regions/eu/official.toml"),
    ),
    ("eu/tax.toml", include_str!("../assets/regions/eu/tax.toml")),
    (
        "eu/fr/official.toml",
        include_str!("../assets/regions/eu/fr/official.toml"),
    ),
    (
        "eu/fr/tax.toml",
        include_str!("../assets/regions/eu/fr/tax.toml"),
    ),
    (
        "us/official.toml",
        include_str!("../assets/regions/us/official.toml"),
    ),
    ("us/tax.toml", include_str!("../assets/regions/us/tax.toml")),
    (
        "vn/official.toml",
        include_str!("../assets/regions/vn/official.toml"),
    ),
    ("vn/tax.toml", include_str!("../assets/regions/vn/tax.toml")),
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PackIssue {
    /// Region code, or the pack-relative path when the directory never became a region.
    pub path: String,
    pub code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct PackStatus {
    pub last_loaded_at: Option<u64>,
    pub issues: Vec<PackIssue>,
    pub regions_registered: usize,
    pub sources_registered: usize,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct RegionFile {
    code: Option<String>,
    name_vi: String,
    name_en: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    timezone: String,
    #[serde(default)]
    language: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SourcesFile {
    #[serde(default)]
    source: Vec<SourceFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceFile {
    slug: String,
    name: String,
    url: String,
    tier: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default = "default_interval")]
    interval_minutes: i64,
    /// Credit line the publisher's terms require, e.g. "Nguồn: VnExpress".
    #[serde(default)]
    attribution: Option<String>,
}

fn default_interval() -> i64 {
    60
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct OfficialFile {
    #[serde(default)]
    body: Vec<BodyFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyFile {
    code: String,
    name_vi: String,
    name_en: String,
    url: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    attribution: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct TaxFile {
    #[serde(default)]
    source: Vec<TaxSourceFile>,
    #[serde(default)]
    deadline: Vec<DeadlineFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaxSourceFile {
    code: String,
    name_vi: String,
    name_en: String,
    url: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    attribution: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeadlineFile {
    code: String,
    source: String,
    name_vi: String,
    name_en: String,
    /// `monthly` | `quarterly` | `yearly`
    cadence: String,
    #[serde(default)]
    month: Option<i64>,
    #[serde(default)]
    day: Option<i64>,
    #[serde(default)]
    weekend_rule: Option<String>,
    #[serde(default)]
    lead_days: Option<i64>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// A pack must state in the file itself whether the rule was checked.
    #[serde(default)]
    verified: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackBody {
    pub code: String,
    pub name_vi: String,
    pub name_en: String,
    pub url: String,
    pub language: String,
    pub attribution: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackTaxSource {
    pub code: String,
    pub name_vi: String,
    pub name_en: String,
    pub url: String,
    pub kind: String,
    pub attribution: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PackDeadline {
    pub code: String,
    pub source: String,
    pub name_vi: String,
    pub name_en: String,
    pub cadence: String,
    pub month: Option<i64>,
    pub day: Option<i64>,
    pub weekend_rule: String,
    pub lead_days: i64,
    pub url: String,
    pub note: String,
    pub verified: bool,
}

#[derive(Debug, Clone)]
pub struct PackSource {
    pub slug: String,
    pub name: String,
    pub url: String,
    pub tier: String,
    pub language: String,
    pub topic: Option<String>,
    pub interval_minutes: i64,
    pub attribution: String,
}

#[derive(Debug, Clone)]
pub struct PackRegion {
    pub code: String,
    pub parent: String,
    pub label_vi: String,
    pub label_en: String,
    pub currency: String,
    pub timezone: String,
    pub language: String,
    pub sources: Vec<PackSource>,
    pub official_bodies: Vec<PackBody>,
    pub tax_sources: Vec<PackTaxSource>,
    pub tax_deadlines: Vec<PackDeadline>,
}

#[derive(Debug, Default)]
pub struct PackScan {
    pub regions: Vec<PackRegion>,
    pub issues: Vec<PackIssue>,
}

fn issue(path: &str, code: &str, detail: impl Into<Option<String>>) -> PackIssue {
    PackIssue {
        path: path.to_string(),
        code: code.to_string(),
        detail: detail.into().map(|detail| {
            let mut detail = detail;
            detail.truncate(240);
            detail
        }),
    }
}

fn clean(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.chars().count() <= max && !value.chars().any(char::is_control)
}

fn valid_timezone(value: &str) -> bool {
    value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'_' | b'+' | b'-'))
}

fn valid_currency(value: &str) -> bool {
    value.len() <= 8 && value.bytes().all(|c| c.is_ascii_alphabetic())
}

/// Writes built-in pack files that are missing. An existing file is **never**
/// overwritten, so owner edits survive upgrades; a file added by a newer release (for
/// example `tax.toml` in P3) is still created on an existing installation.
pub fn seed_builtin(dir: &FsPath) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    for (relative, contents) in BUILTIN {
        let target = dir.join(relative);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, contents)?;
    }
    Ok(())
}

fn parse_region_file(dir: &FsPath, label: &str, issues: &mut Vec<PackIssue>) -> Option<RegionFile> {
    let text = match fs::read_to_string(dir.join("region.toml")) {
        Ok(text) => text,
        Err(err) => {
            issues.push(issue(label, "unreadable_file", Some(err.to_string())));
            return None;
        }
    };
    match toml::from_str::<RegionFile>(&text) {
        Ok(file) => Some(file),
        Err(err) => {
            issues.push(issue(label, "syntax_error", Some(err.to_string())));
            None
        }
    }
}

fn parse_sources_file(dir: &FsPath, label: &str, issues: &mut Vec<PackIssue>) -> Vec<PackSource> {
    let path = dir.join("sources.toml");
    if !path.exists() {
        return Vec::new();
    }
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            issues.push(issue(label, "unreadable_file", Some(err.to_string())));
            return Vec::new();
        }
    };
    let file = match toml::from_str::<SourcesFile>(&text) {
        Ok(file) => file,
        Err(err) => {
            issues.push(issue(label, "syntax_error", Some(err.to_string())));
            return Vec::new();
        }
    };
    let mut seen = HashSet::new();
    let mut sources = Vec::new();
    for source in file.source {
        let entry = PackSource {
            slug: source.slug.trim().to_string(),
            name: source.name.trim().to_string(),
            url: source.url.trim().to_string(),
            tier: source.tier.trim().to_ascii_uppercase(),
            language: source.language.trim().to_string(),
            topic: source
                .topic
                .map(|topic| topic.trim().to_string())
                .filter(|topic| !topic.is_empty()),
            interval_minutes: source.interval_minutes,
            attribution: source
                .attribution
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default(),
        };
        if !catalog::valid_slug(&entry.slug)
            || !clean(&entry.name, 120)
            || !catalog::valid_source_url(&entry.url)
            || !matches!(entry.tier.as_str(), "T1" | "T2" | "T3" | "T4")
            || !(5..=10_080).contains(&entry.interval_minutes)
            || entry.language.chars().count() > 16
            || entry.attribution.chars().count() > 200
        {
            issues.push(issue(label, "invalid_source", Some(entry.slug)));
            continue;
        }
        if !seen.insert(entry.slug.clone()) {
            issues.push(issue(label, "duplicate_source", Some(entry.slug)));
            continue;
        }
        sources.push(entry);
    }
    sources
}

/// Reads an optional file and parses it. A missing file is not an error: a pack may
/// declare only sources, only deadlines, or any mix.
fn read_optional<T: for<'de> Deserialize<'de> + Default>(
    dir: &FsPath,
    name: &str,
    issues: &mut Vec<PackIssue>,
) -> Option<T> {
    let path = dir.join(name);
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(&path) {
        Ok(text) => match toml::from_str::<T>(&text) {
            Ok(file) => Some(file),
            Err(err) => {
                issues.push(issue(name, "syntax_error", Some(err.to_string())));
                None
            }
        },
        Err(err) => {
            issues.push(issue(name, "unreadable_file", Some(err.to_string())));
            None
        }
    }
}

fn parse_official_file(dir: &FsPath, issues: &mut Vec<PackIssue>) -> Vec<PackBody> {
    let Some(file) = read_optional::<OfficialFile>(dir, "official.toml", issues) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut bodies = Vec::new();
    for body in file.body {
        let entry = PackBody {
            code: body.code.trim().to_string(),
            name_vi: body.name_vi.trim().to_string(),
            name_en: body.name_en.trim().to_string(),
            url: body.url.trim().to_string(),
            language: body.language.trim().to_string(),
            attribution: body
                .attribution
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default(),
        };
        if !catalog::valid_slug(&entry.code)
            || !clean(&entry.name_vi, 120)
            || !clean(&entry.name_en, 120)
            || !catalog::valid_source_url(&entry.url)
            || entry.language.chars().count() > 16
            || entry.attribution.chars().count() > 200
        {
            issues.push(issue("official.toml", "invalid_body", Some(entry.code)));
            continue;
        }
        if !seen.insert(entry.code.clone()) {
            issues.push(issue("official.toml", "duplicate_code", Some(entry.code)));
            continue;
        }
        bodies.push(entry);
    }
    bodies
}

fn parse_tax_file(
    dir: &FsPath,
    issues: &mut Vec<PackIssue>,
) -> (Vec<PackTaxSource>, Vec<PackDeadline>) {
    let Some(file) = read_optional::<TaxFile>(dir, "tax.toml", issues) else {
        return (Vec::new(), Vec::new());
    };
    let mut sources = Vec::new();
    let mut seen_sources = HashSet::new();
    for source in file.source {
        let entry = PackTaxSource {
            code: source.code.trim().to_string(),
            name_vi: source.name_vi.trim().to_string(),
            name_en: source.name_en.trim().to_string(),
            url: source.url.trim().to_string(),
            kind: source
                .kind
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "authority".into()),
            attribution: source
                .attribution
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_default(),
        };
        if !catalog::valid_slug(&entry.code)
            || !clean(&entry.name_vi, 120)
            || !clean(&entry.name_en, 120)
            || !catalog::valid_source_url(&entry.url)
            || !matches!(entry.kind.as_str(), "authority" | "guidance" | "calendar")
            || entry.attribution.chars().count() > 200
        {
            issues.push(issue("tax.toml", "invalid_tax_source", Some(entry.code)));
            continue;
        }
        if !seen_sources.insert(entry.code.clone()) {
            issues.push(issue("tax.toml", "duplicate_code", Some(entry.code)));
            continue;
        }
        sources.push(entry);
    }
    let mut deadlines = Vec::new();
    let mut seen_deadlines = HashSet::new();
    for deadline in file.deadline {
        let entry = PackDeadline {
            code: deadline.code.trim().to_string(),
            source: deadline.source.trim().to_string(),
            name_vi: deadline.name_vi.trim().to_string(),
            name_en: deadline.name_en.trim().to_string(),
            cadence: deadline.cadence.trim().to_ascii_lowercase(),
            month: deadline.month,
            day: deadline.day,
            weekend_rule: deadline
                .weekend_rule
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "none".into()),
            lead_days: deadline.lead_days.unwrap_or(14),
            url: deadline
                .url
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            note: deadline
                .note
                .map(|value| value.trim().to_string())
                .unwrap_or_default(),
            verified: deadline.verified,
        };
        // Validation is intentionally strict: a malformed rule would produce a wrong
        // legal deadline, which is worse than an empty calendar.
        let valid = catalog::valid_slug(&entry.code)
            && clean(&entry.name_vi, 120)
            && clean(&entry.name_en, 120)
            && matches!(entry.cadence.as_str(), "monthly" | "quarterly" | "yearly")
            && matches!(
                entry.weekend_rule.as_str(),
                "none" | "next_business_day" | "previous_business_day"
            )
            && (1..=366).contains(&entry.lead_days)
            && entry
                .day
                .is_none_or(|day| (1..=28).contains(&day) || day == 31)
            && match entry.cadence.as_str() {
                "yearly" => entry.month.is_some_and(|month| (1..=12).contains(&month)),
                "quarterly" => {
                    entry.month.is_none() || entry.month.is_some_and(|m| (1..=12).contains(&m))
                }
                _ => entry.month.is_none(),
            }
            && (entry.url.is_empty() || catalog::valid_source_url(&entry.url))
            && entry.note.chars().count() <= 300;
        if !valid {
            issues.push(issue("tax.toml", "invalid_deadline", Some(entry.code)));
            continue;
        }
        if !seen_sources.contains(&entry.source) {
            issues.push(issue("tax.toml", "unknown_tax_source", Some(entry.source)));
            continue;
        }
        if !seen_deadlines.insert(entry.code.clone()) {
            issues.push(issue("tax.toml", "duplicate_code", Some(entry.code)));
            continue;
        }
        deadlines.push(entry);
    }
    (sources, deadlines)
}

/// True when `path` is still inside `root` after resolving symlinks and `..`.
fn inside_root(root: &FsPath, path: &FsPath) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => false,
    }
}

fn walk(
    root: &FsPath,
    dir: &FsPath,
    relative: &str,
    parent: &str,
    depth: usize,
    scan: &mut PackScan,
) {
    if depth > MAX_DEPTH {
        scan.issues.push(issue(relative, "too_deep", None));
        return;
    }
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(entries) => entries.filter_map(Result::ok).collect(),
        Err(err) => {
            scan.issues.push(issue(
                relative,
                "unreadable_directory",
                Some(err.to_string()),
            ));
            return;
        }
    };
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with('_') {
            continue;
        }
        let child_relative = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            scan.issues
                .push(issue(&child_relative, "symlink_skipped", None));
            continue;
        }
        if !file_type.is_dir() {
            continue;
        }
        let child = entry.path();
        if !inside_root(root, &child) {
            scan.issues
                .push(issue(&child_relative, "path_escape", None));
            continue;
        }
        if !child.join("region.toml").exists() {
            if child.join("sources.toml").exists() {
                scan.issues
                    .push(issue(&child_relative, "missing_region_file", None));
            }
            report_orphans(root, &child, &child_relative, depth + 1, scan);
            continue;
        }
        let Some(file) = parse_region_file(&child, &child_relative, &mut scan.issues) else {
            continue;
        };
        let code = name.to_ascii_lowercase();
        if let Some(declared) = file.code.as_deref().map(str::trim)
            && declared != code
        {
            scan.issues.push(issue(
                &child_relative,
                "code_mismatch",
                Some(declared.to_string()),
            ));
            continue;
        }
        if !catalog::valid_slug(&code)
            || !clean(&file.name_vi, 120)
            || !clean(&file.name_en, 120)
            || !valid_currency(file.currency.trim())
            || !valid_timezone(file.timezone.trim())
            || file.language.chars().count() > 16
        {
            scan.issues
                .push(issue(&child_relative, "invalid_region", None));
            continue;
        }
        if code == ROOT_CODE || scan.regions.iter().any(|region| region.code == code) {
            scan.issues
                .push(issue(&child_relative, "duplicate_region_code", Some(code)));
            continue;
        }
        let sources = parse_sources_file(&child, &child_relative, &mut scan.issues);
        let official_bodies = parse_official_file(&child, &mut scan.issues);
        let (tax_sources, tax_deadlines) = parse_tax_file(&child, &mut scan.issues);
        scan.regions.push(PackRegion {
            code: code.clone(),
            parent: parent.to_string(),
            label_vi: file.name_vi.trim().to_string(),
            label_en: file.name_en.trim().to_string(),
            currency: file.currency.trim().to_ascii_uppercase(),
            timezone: file.timezone.trim().to_string(),
            language: file.language.trim().to_string(),
            sources,
            official_bodies,
            tax_sources,
            tax_deadlines,
        });
        walk(root, &child, &child_relative, &code, depth + 1, scan);
    }
}

/// A branch without `region.toml` is not a region, so anything beneath it is an
/// orphan and is reported rather than silently attached to a grandparent.
fn report_orphans(root: &FsPath, dir: &FsPath, relative: &str, depth: usize, scan: &mut PackScan) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || name.starts_with('_') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let child = entry.path();
        if !inside_root(root, &child) {
            continue;
        }
        let child_relative = format!("{relative}/{name}");
        if child.join("region.toml").exists() {
            scan.issues
                .push(issue(&child_relative, "region_orphan", None));
        }
        report_orphans(root, &child, &child_relative, depth + 1, scan);
    }
}

/// Scans a pack directory. The root itself is `global`; its `region.toml` supplies
/// the labels but cannot rename or reparent it.
pub fn scan(root: &FsPath) -> PackScan {
    let mut scan = PackScan::default();
    let Ok(root) = root.canonicalize() else {
        scan.issues
            .push(issue("", "pack_missing", Some(root.display().to_string())));
        scan.regions.push(global_region());
        return scan;
    };
    let root_file = parse_region_file(&root, "", &mut scan.issues);
    let mut region = global_region();
    if let Some(file) = root_file {
        if file.code.as_deref().is_some_and(|c| c.trim() != ROOT_CODE) {
            scan.issues.push(issue(
                "region.toml",
                "code_mismatch",
                Some(file.code.unwrap_or_default()),
            ));
        }
        if clean(&file.name_vi, 120) {
            region.label_vi = file.name_vi.trim().into();
        }
        if clean(&file.name_en, 120) {
            region.label_en = file.name_en.trim().into();
        }
        region.currency = file.currency.trim().to_ascii_uppercase();
        region.timezone = file.timezone.trim().to_string();
        region.language = file.language.trim().to_string();
        region.sources = parse_sources_file(&root, "sources.toml", &mut scan.issues);
        region.official_bodies = parse_official_file(&root, &mut scan.issues);
        let (tax_sources, tax_deadlines) = parse_tax_file(&root, &mut scan.issues);
        region.tax_sources = tax_sources;
        region.tax_deadlines = tax_deadlines;
    }
    scan.regions.push(region);
    walk(&root, &root, "", ROOT_CODE, 0, &mut scan);
    if scan.issues.len() > MAX_ISSUES {
        scan.issues.truncate(MAX_ISSUES);
        scan.issues.push(issue("", "too_many_issues", None));
    }
    scan
}

fn global_region() -> PackRegion {
    PackRegion {
        code: ROOT_CODE.into(),
        parent: String::new(),
        label_vi: "Toàn cầu".into(),
        label_en: "Global".into(),
        currency: String::new(),
        timezone: "UTC".into(),
        language: String::new(),
        sources: Vec::new(),
        official_bodies: Vec::new(),
        tax_sources: Vec::new(),
        tax_deadlines: Vec::new(),
    }
}

// ---------- tree queries ----------

pub fn region_enabled(db: &db::Database, code: &str) -> rusqlite::Result<bool> {
    let value: Option<i64> = db
        .conn
        .query_row("SELECT enabled FROM regions WHERE code=?1", [code], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(value == Some(1))
}

/// Effective region set: every enabled region plus all of its ancestors.
///
/// With `code`, the set is that region and its ancestors; the caller must have
/// validated that the region exists and is enabled.
pub fn effective(db: &db::Database, code: Option<&str>) -> rusqlite::Result<Vec<String>> {
    let mut stmt = db.conn.prepare(
        "WITH RECURSIVE up(code) AS (
           SELECT code FROM regions
            WHERE (?1 IS NULL AND enabled=1) OR (?1 IS NOT NULL AND code=?1)
           UNION
           SELECT r.parent_code FROM regions r JOIN up ON r.code=up.code
            WHERE r.parent_code IS NOT NULL
         ) SELECT code FROM up ORDER BY code",
    )?;
    let rows = stmt.query_map([code], |r| r.get(0))?;
    rows.collect()
}

fn find_cycle(edges: &HashMap<String, Option<String>>) -> Option<String> {
    for start in edges.keys() {
        let mut seen = HashSet::new();
        let mut current = Some(start.clone());
        while let Some(code) = current {
            if !seen.insert(code.clone()) {
                return Some(start.clone());
            }
            current = edges.get(&code).cloned().flatten();
        }
    }
    None
}

// ---------- applying a scan ----------

/// Applies a scan in one transaction. Existing rows are never deleted: a pack that
/// loses a directory keeps its previous entries, and everything new starts disabled.
pub fn apply(
    db: &mut db::Database,
    scan: &PackScan,
) -> anyhow::Result<(usize, usize, Vec<PackIssue>)> {
    let mut issues = scan.issues.clone();
    let tx = db.conn.transaction()?;
    let mut existing: HashMap<String, Option<String>> = HashMap::new();
    {
        let mut stmt = tx.prepare("SELECT code,parent_code FROM regions")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
        for row in rows {
            let (code, parent) = row?;
            existing.insert(code, parent);
        }
    }
    let mut edges = existing.clone();
    for region in &scan.regions {
        edges.insert(
            region.code.clone(),
            if region.code == ROOT_CODE {
                None
            } else {
                Some(region.parent.clone())
            },
        );
    }
    if let Some(code) = find_cycle(&edges) {
        issues.push(issue(&code, "region_cycle", None));
        return Ok((0, 0, issues));
    }
    let mut accepted = Vec::new();
    for region in &scan.regions {
        if region.code != ROOT_CODE && !edges.contains_key(&region.parent) {
            issues.push(issue(
                &region.code,
                "region_orphan",
                Some(region.parent.clone()),
            ));
            continue;
        }
        // Reusing an existing code under a different parent is a conflict, not a
        // silent reparent that would move articles between trees.
        if region.code != ROOT_CODE
            && existing
                .get(&region.code)
                .is_some_and(|parent| parent.as_deref() != Some(region.parent.as_str()))
        {
            issues.push(issue(
                &region.code,
                "region_conflict",
                Some(region.parent.clone()),
            ));
            continue;
        }
        accepted.push(region);
    }
    let mut accepted_codes = Vec::new();
    for region in &accepted {
        accepted_codes.push(region.code.clone());
        tx.execute(
            "INSERT INTO regions(code,parent_code,label_vi,label_en,currency,timezone,language,enabled,builtin,managed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,1)
             ON CONFLICT(code) DO UPDATE SET parent_code=excluded.parent_code,label_vi=excluded.label_vi,
               label_en=excluded.label_en,currency=excluded.currency,timezone=excluded.timezone,
               language=excluded.language,managed=1,updated_at=CURRENT_TIMESTAMP",
            params![
                region.code,
                if region.code == ROOT_CODE {
                    None
                } else {
                    Some(region.parent.as_str())
                },
                region.label_vi,
                region.label_en,
                region.currency,
                region.timezone,
                region.language,
                region.code == ROOT_CODE,
            ],
        )?;
        for source in &region.sources {
            let pack_key = format!("{}/{}", region.code, source.slug);
            let label = format!("{}/{}", region.code, source.slug);
            if let Some(topic) = source.topic.as_deref() {
                let known: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM topics WHERE slug=?1)",
                    [topic],
                    |r| r.get(0),
                )?;
                if !known {
                    issues.push(issue(&label, "invalid_topic", Some(topic.to_string())));
                    continue;
                }
            }
            let existing_id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM sources WHERE pack_key=?1",
                    [&pack_key],
                    |r| r.get(0),
                )
                .optional()?;
            let manual_clash: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sources WHERE pack_key IS NULL AND (url=?1 OR slug=?2))",
                params![source.url, format!("pack-{}", pack_key.replace('/', "-"))],
                |r| r.get(0),
            )?;
            if manual_clash && existing_id.is_none() {
                issues.push(issue(&label, "source_conflict", Some(source.url.clone())));
                continue;
            }
            match existing_id {
                Some(id) => {
                    tx.execute(
                        "UPDATE sources SET name=?2,url=?3,tier=?4,language=?5,topic_slug=?6,fetch_interval_minutes=?7,region=?8,attribution=?9 WHERE id=?1",
                        params![
                            id,
                            source.name,
                            source.url,
                            source.tier,
                            source.language,
                            source.topic,
                            source.interval_minutes,
                            region.code,
                            source.attribution
                        ],
                    )?;
                }
                None => {
                    let slug = unique_pack_slug(&tx, &pack_key)?;
                    tx.execute(
                        "INSERT INTO sources(slug,name,url,tier,enabled,language,region,topic_slug,fetch_interval_minutes,pack_key,attribution)
                         VALUES (?1,?2,?3,?4,0,?5,?6,?7,?8,?9,?10)",
                        params![
                            slug,
                            source.name,
                            source.url,
                            source.tier,
                            source.language,
                            region.code,
                            source.topic,
                            source.interval_minutes,
                            pack_key,
                            source.attribution
                        ],
                    )?;
                }
            }
        }
    }
    // Official bodies, tax sources and recurring deadlines follow the same rule: an
    // upsert never enables a row, and an existing `enabled` choice is preserved by the
    // ON CONFLICT clause (it only refreshes the descriptive fields).
    for region in accepted.iter() {
        for body in &region.official_bodies {
            tx.execute(
                "INSERT INTO official_bodies(pack_key,region,code,name_vi,name_en,url,language,enabled,attribution)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8)
                 ON CONFLICT(pack_key) DO UPDATE SET region=excluded.region,name_vi=excluded.name_vi,
                   name_en=excluded.name_en,url=excluded.url,language=excluded.language,
                   attribution=excluded.attribution",
                params![
                    format!("{}/{}", region.code, body.code),
                    region.code,
                    body.code,
                    body.name_vi,
                    body.name_en,
                    body.url,
                    body.language,
                    body.attribution
                ],
            )?;
        }
        for source in &region.tax_sources {
            tx.execute(
                "INSERT INTO tax_sources(pack_key,region,code,name_vi,name_en,url,kind,enabled,attribution)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8)
                 ON CONFLICT(pack_key) DO UPDATE SET region=excluded.region,name_vi=excluded.name_vi,
                   name_en=excluded.name_en,url=excluded.url,kind=excluded.kind,
                   attribution=excluded.attribution",
                params![
                    format!("{}/{}", region.code, source.code),
                    region.code,
                    source.code,
                    source.name_vi,
                    source.name_en,
                    source.url,
                    source.kind,
                    source.attribution
                ],
            )?;
        }
        for deadline in &region.tax_deadlines {
            tx.execute(
                "INSERT INTO tax_deadlines(pack_key,region,source_code,code,name_vi,name_en,cadence,
                    month,day,weekend_rule,lead_days,url,note,verified,enabled)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,0)
                 ON CONFLICT(pack_key) DO UPDATE SET region=excluded.region,name_vi=excluded.name_vi,
                   name_en=excluded.name_en,cadence=excluded.cadence,month=excluded.month,
                   day=excluded.day,weekend_rule=excluded.weekend_rule,lead_days=excluded.lead_days,
                   url=excluded.url,note=excluded.note,verified=excluded.verified",
                params![
                    format!("{}/{}", region.code, deadline.code),
                    region.code,
                    deadline.source,
                    deadline.code,
                    deadline.name_vi,
                    deadline.name_en,
                    deadline.cadence,
                    deadline.month,
                    deadline.day,
                    deadline.weekend_rule,
                    deadline.lead_days,
                    deadline.url,
                    deadline.note,
                    deadline.verified
                ],
            )?;
        }
    }
    // P1 stored `sources.region` as free text, so a value that never became a region
    // code is moved to `global`; its items follow the source, which keeps the reader
    // from hiding them under an unknown code.
    tx.execute(
        "UPDATE sources SET region=?1 WHERE region IS NULL OR region='' OR region NOT IN (SELECT code FROM regions)",
        [ROOT_CODE],
    )?;
    tx.execute(
        "UPDATE articles SET region_code=COALESCE((SELECT s.region FROM sources s WHERE s.id=articles.source_id),?1) WHERE region_code IS NULL",
        [ROOT_CODE],
    )?;
    tx.execute(
        "UPDATE articles SET region_code=(SELECT s.region FROM sources s WHERE s.id=articles.source_id)
          WHERE source_id IS NOT NULL
            AND EXISTS (SELECT 1 FROM sources s WHERE s.id=articles.source_id AND s.region<>COALESCE(articles.region_code,''))",
        [],
    )?;
    let sources_registered: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sources WHERE pack_key IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    tx.commit()?;
    Ok((accepted_codes.len(), sources_registered as usize, issues))
}

/// Pack slugs are namespaced so they can never collide with a hand-added source.
fn unique_pack_slug(tx: &rusqlite::Transaction<'_>, pack_key: &str) -> rusqlite::Result<String> {
    let base = format!("pack-{}", pack_key.replace('/', "-"));
    let mut candidate = base.clone();
    let mut suffix = 1;
    loop {
        let taken: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sources WHERE slug=?1)",
            [&candidate],
            |r| r.get(0),
        )?;
        if !taken {
            return Ok(candidate);
        }
        suffix += 1;
        candidate = format!("{base}-{suffix}");
    }
}

/// Reload using an explicit directory, which tests point at a temporary pack.
#[cfg(test)]
pub fn reload_with_dir(state: &SharedState, dir: &FsPath) -> PackStatus {
    let scan = scan(dir);
    match apply(&mut state.db.lock().expect("database lock poisoned"), &scan) {
        Ok((regions, sources, issues)) => PackStatus {
            last_loaded_at: Some(now()),
            issues,
            regions_registered: regions,
            sources_registered: sources,
        },
        Err(err) => PackStatus {
            last_loaded_at: Some(now()),
            issues: vec![issue("", "apply_failed", Some(err.to_string()))],
            regions_registered: 0,
            sources_registered: 0,
        },
    }
}

/// Scans the configured pack directory, applies it and records the outcome. The
/// filesystem is read without holding the database lock.
pub fn reload(state: &SharedState) -> PackStatus {
    if let Err(err) = seed_builtin(&state.cfg.regions_dir) {
        tracing::warn!(%err, "could not seed region pack");
    }
    let scan = scan(&state.cfg.regions_dir);
    let found_issues = scan.issues.len();
    let found_regions = scan.regions.len();
    let result = apply(&mut state.db.lock().expect("database lock poisoned"), &scan);
    let status = match result {
        Ok((regions, sources, issues)) => PackStatus {
            last_loaded_at: Some(now()),
            issues,
            regions_registered: regions,
            sources_registered: sources,
        },
        Err(err) => {
            tracing::error!(%err, "region pack apply failed");
            PackStatus {
                last_loaded_at: Some(now()),
                issues: vec![issue("", "apply_failed", Some(err.to_string()))],
                regions_registered: 0,
                sources_registered: 0,
            }
        }
    };
    tracing::info!(
        regions = found_regions,
        sources = status.sources_registered,
        scan_issues = found_issues,
        apply_issues = status.issues.len(),
        "region pack loaded"
    );
    *state.pack.lock().expect("pack lock poisoned") = status.clone();
    status
}

// ---------- endpoints ----------

#[derive(Debug, Serialize)]
pub struct RegionRow {
    code: String,
    parent_code: Option<String>,
    label_vi: String,
    label_en: String,
    currency: String,
    timezone: String,
    language: String,
    enabled: bool,
    builtin: bool,
    managed: bool,
    source_count: i64,
}

fn rows(db: &db::Database, enabled_only: bool) -> rusqlite::Result<Vec<RegionRow>> {
    let mut stmt = db.conn.prepare(
        "SELECT r.code,r.parent_code,r.label_vi,r.label_en,r.currency,r.timezone,r.language,r.enabled,r.builtin,r.managed,
                (SELECT COUNT(*) FROM sources s WHERE s.region=r.code)
         FROM regions r WHERE (?1=0 OR r.enabled=1)",
    )?;
    let rows = stmt.query_map([enabled_only], |r| {
        Ok(RegionRow {
            code: r.get(0)?,
            parent_code: r.get(1)?,
            label_vi: r.get(2)?,
            label_en: r.get(3)?,
            currency: r.get(4)?,
            timezone: r.get(5)?,
            language: r.get(6)?,
            enabled: r.get::<_, i64>(7)? != 0,
            builtin: r.get::<_, i64>(8)? != 0,
            managed: r.get::<_, i64>(9)? != 0,
            source_count: r.get(10)?,
        })
    })?;
    let mut data = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    // Tree order: each region follows its parent, so the list reads like the pack
    // directory tree rather than alphabetically.
    let parents: HashMap<String, Option<String>> = data
        .iter()
        .map(|row| (row.code.clone(), row.parent_code.clone()))
        .collect();
    let path_of = |code: &str| -> String {
        let mut parts = vec![code.to_string()];
        let mut current = parents.get(code).cloned().flatten();
        let mut guard = 0;
        while let Some(parent) = current {
            parts.push(parent.clone());
            guard += 1;
            if guard > MAX_DEPTH {
                break;
            }
            current = parents.get(&parent).cloned().flatten();
        }
        parts.reverse();
        parts.join("/")
    };
    data.sort_by_key(|row| path_of(&row.code));
    Ok(data)
}

pub(crate) fn payload(db: &db::Database, enabled_only: bool) -> anyhow::Result<serde_json::Value> {
    let data = rows(db, enabled_only)?;
    let effective = effective(db, None)?;
    Ok(serde_json::json!({ "data": data, "effective": effective }))
}

/// Admin view: tree, effective set and the last pack scan (including issues).
pub async fn list(State(state): State<SharedState>) -> Response {
    let result = payload(&state.db.lock().expect("database lock poisoned"), false);
    match result {
        Ok(mut value) => {
            value["pack"] =
                serde_json::to_value(state.pack.lock().expect("pack lock poisoned").clone())
                    .unwrap_or_default();
            Json(value).into_response()
        }
        Err(err) => internal_error(err),
    }
}

/// Reader/API view: only enabled regions.
pub async fn list_enabled(State(state): State<SharedState>) -> Response {
    match payload(&state.db.lock().expect("database lock poisoned"), true) {
        Ok(value) => Json(value).into_response(),
        Err(err) => internal_error(err),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionToggle {
    enabled: bool,
}

pub async fn set_enabled(
    State(state): State<SharedState>,
    Path(code): Path<String>,
    Json(input): Json<RegionToggle>,
) -> Response {
    let db = state.db.lock().expect("database lock poisoned");
    let parent: Option<Option<String>> = match db
        .conn
        .query_row(
            "SELECT parent_code FROM regions WHERE code=?1",
            [&code],
            |r| r.get(0),
        )
        .optional()
    {
        Ok(value) => value,
        Err(err) => return internal_error(err),
    };
    let Some(parent) = parent else {
        return failure(StatusCode::NOT_FOUND, "not_found");
    };
    if code == ROOT_CODE && !input.enabled {
        return failure(StatusCode::CONFLICT, "region_required");
    }
    if input.enabled {
        if let Some(parent) = parent
            && !matches!(region_enabled(&db, &parent), Ok(true))
        {
            return failure(StatusCode::CONFLICT, "invalid_parent_region");
        }
    } else {
        // A region cannot be turned off while anything below it in the tree (not
        // only a direct child) is still enabled.
        let enabled_children: i64 = match db.conn.query_row(
            "WITH RECURSIVE below(code) AS (
               SELECT code FROM regions WHERE parent_code=?1
               UNION SELECT r.code FROM regions r JOIN below ON r.parent_code=below.code)
             SELECT COUNT(*) FROM regions WHERE code IN (SELECT code FROM below) AND enabled=1",
            [&code],
            |r| r.get(0),
        ) {
            Ok(count) => count,
            Err(err) => return internal_error(err),
        };
        if enabled_children > 0 {
            return failure(StatusCode::CONFLICT, "region_in_use");
        }
    }
    let result = db.conn.execute(
        "UPDATE regions SET enabled=?2, updated_at=CURRENT_TIMESTAMP WHERE code=?1",
        params![code, input.enabled],
    );
    drop(db);
    match result {
        Ok(0) => failure(StatusCode::NOT_FOUND, "not_found"),
        Ok(_) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => internal_error(err),
    }
}

pub async fn reload_handler(State(state): State<SharedState>) -> Response {
    let outcome = tokio::task::spawn_blocking({
        let state = state.clone();
        move || reload(&state)
    })
    .await;
    match outcome {
        Ok(status) => Json(serde_json::json!({ "data": status })).into_response(),
        Err(err) => internal_error(err),
    }
}

#[cfg(test)]
#[path = "regions_tests.rs"]
pub(crate) mod tests;

# LNC Newsman API — P1 collector + P1.5 regions (local)

P1 acceptance against real feeds is still open. P1.5 region support is implemented and covered by loopback tests and a local browser smoke run; it is not owner-accepted and no real feed was fetched. Successful API responses and application errors are JSON. Application errors use `{ "ok": false, "error": "code" }`; malformed JSON or extractor failures may return framework-generated 4xx responses. No cross-origin CORS is enabled. Non-GET/HEAD/OPTIONS requests require `X-Requested-With: XMLHttpRequest`, including login and logout.

## Public

| Method | Path | Purpose |
|---|---|---|
| GET | `/health` | Liveness, no authentication |
| GET | `/api/auth/status` | Version, management-key presence and current cookie state |
| POST | `/api/auth/login` | JSON `{ "key": "..." }`; issues a 12-hour management cookie |
| POST | `/api/auth/logout` | Revokes the matching server-side session and clears the cookie |
| GET | `/api/app-info` | App name, version and surface mode |
| GET | `/login.html` | UI entry |

## Management session only

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/dashboard` | Enabled topics plus `stats` (collector switch, item counts, enabled/paused sources, per-topic counts). Optional `region`: article counters (`articles_total`, `articles_24h`, `topic_counts`) then follow that region and its ancestors, like the article list; 400 `invalid_region` if the region is unknown or disabled. Source counters stay global |
| GET | `/api/settings` | Global language, reader-mode defaults, collector/fulltext/connector background switches, focus profile and brief schedule |
| PATCH | `/api/settings` | Partial JSON: `language` = `vi`/`en`, `default_reader_mode` = `paper`/`sepia`/`night`, `collector_enabled` = boolean (default `false`), `fulltext_auto_enabled` = boolean (default `true`), `connectors_auto_enabled` = boolean (default `false`), `focus_profile` (≤1000 chars), `brief_morning_time` / `brief_evening_time` (`HH:MM` or `""`), `brief_timezone` (IANA zone) |
| GET | `/api/admin/topics` | List enabled and disabled topics |
| POST | `/api/admin/topics` | Create a topic; returns 201 |
| PUT | `/api/admin/topics/{slug}` | Replace topic fields |
| GET | `/api/admin/sources` | List enabled and disabled sources |
| POST | `/api/admin/sources` | Create a source; returns 201 |
| PUT | `/api/admin/sources/{id}` | Replace source fields by numeric ID |
| GET | `/api/admin/sources/presets` | Curated official/public RSS/Atom presets with `installed`, `region_enabled` and required `attribution`. Zero network calls |
| POST | `/api/admin/sources/presets/{slug}/add` | Register one curated preset **disabled** (`enabled = false`) with its required `attribution`; returns 201 `{ data: { id, slug, enabled: false } }`, 409 `duplicate_slug`/`duplicate_source`, or 404 `not_found`. Zero network calls |
| POST | `/api/admin/sources/preview` | `{ "url": "..." }`: fetch and parse once, store nothing; returns `items_seen` and up to 5 sample titles. Failures return 502 `{ "error": "preview_failed", "reason": "..." }` |
| POST | `/api/admin/sources/{id}/fetch` | Fetch one source now (also works for a disabled source); returns `status` (`ok`/`not_modified`/`error`), `items_seen`, `items_new`, `error`, `paused`. 409 `fetch_in_progress` if already running |
| POST | `/api/admin/sources/{id}/resume` | Clear an automatic pause and failure count; the source becomes due immediately |
| GET | `/api/admin/sources/{id}/runs` | Last 20 runs (start time, duration, status, counts, error code) |
| GET | `/api/admin/regions` | Full region tree (enabled and disabled), the effective set and the last pack scan: `{ data, effective, pack: { last_loaded_at, issues, regions_registered, sources_registered } }` |
| PUT | `/api/admin/regions/{code}` | Body `{"enabled": boolean}`. Enabling needs an enabled parent; disabling needs every region below it (any depth) to be off; `global` cannot be disabled (`region_required`). Errors: 409 `invalid_parent_region`, `region_in_use`, `region_required`; 404 `not_found` |
| POST | `/api/admin/regions/reload` | Re-scan the pack directory (`REGIONS_DIR`, default `$DATA_DIR/regions`). Registers new regions and sources **disabled**; keeps previous valid rows when a file is broken. Returns the same `pack` object with per-file `issues` (`{ path, code, detail }`) |
| GET | `/api/articles` | Same as `/api/v1/articles` for the reader UI |
| POST | `/api/articles/{id}/extract` | Fetch and extract clean readable text for one visible stored article via the hardened fetcher; updates `summary`, `content_text`, `content_source`, `word_count` and `articles_fts` |
| POST | `/api/admin/articles/backfill` | Optional `{ "limit": 1..20 }`: extract readable web bodies for pending visible articles (`content_source = "none"`), obeying per-host spacing |
| GET | `/api/admin/secrets` | Secret names, key version and `updated_at` only, plus `key_fingerprint`. **Never returns a value.** 503 `secret_store_not_configured` when `LNC_MASTER_KEY` is absent |
| PUT | `/api/admin/secrets/{name}` | `{ "value": "..." }` — write-only; the value is encrypted immediately and cannot be read back |
| DELETE | `/api/admin/secrets/{name}` | Remove a secret |
| POST | `/api/admin/secrets/rewrap` | Re-encrypt every secret under the current key with a new nonce (rotation). Returns `rewrapped`; 409 `secret_rewrap_failed` and **no changes** if any row cannot be opened |
| GET | `/api/admin/connector/writes` | Recent agent writes through the connector: `at`, `client_id`, `app_id`, `tool`, `record_kind`, `record_id`, `outcome`, `note`. Metadata only — no note text |
| GET | `/api/admin/connector/analyses` | Stored briefs/summaries with their `author` label, newest first |
| GET | `/api/admin/connector` | Business MCP Hub connector card: `enabled`, `audience`, `max_scopes`, and the **source** of each value (`env`/`admin`/`unset`) — never a value |
| PATCH | `/api/admin/connector` | Partial JSON `audience`, `max_scopes`; a `null` restores the built-in default. 400 `invalid_connector_audience` / `invalid_connector_scopes` / `unsupported_connector_scope`. The bearer and the delegation secret are **not** accepted here — they are secrets, written through `PUT /api/admin/secrets/{name}` |
| GET | `/api/admin/connectors` | Connector id, provider, region, enabled, health, `last_error`, run times, `requests_today`/`daily_quota`, `requests_last_run`, `needs_secret`, plus `auto_enabled`. No secrets |
| PUT | `/api/admin/connectors/{id}` | Body `{"enabled": boolean}`. Flips a connector's `enabled` switch with zero network calls; enabling checks `connector_region_off`, `connector_not_configured`, `connector_secret_missing` and `connector_secret_unreadable` upfront (409) |
| POST | `/api/admin/connectors/{id}/run` | Run one connector now. Same gates as a scheduled run: 409 with `fetch_in_progress`, `connector_disabled`, `connector_region_off`, `connector_secret_missing`, `connector_not_configured`, `connector_quota_exhausted`, `connector_cache_fresh` or `connector_not_due`; 502 with `connector_fetch_failed`, `connector_provider_error`, `connector_unauthorized`, `connector_invalid_data` |
| POST | `/api/admin/research` | Grounded Hub research/brief run (`{ question, region?, topic?, mode?: "analyse"\|"agent", save_as_brief?: boolean, title? }`). Quotes stored headlines, numeric series, declared deadlines and `focus_profile`; optionally saves to `newsman_analyses` (`stable_key = "analysis-<id>"`) |
| POST | `/api/notes` | Owner feedback note (`{ body, article_id? \| document_id? \| analysis_id? }`), stored with `author_client = NULL` so no delegated MCP agent can edit or delete it |
| DELETE | `/api/notes/{id}` | Owner deletes a stored note by id |
| GET | `/api/admin/legal-sources` | Legal-text sources with the terms verdict each one rests on: `terms_verdict` (`verified`/`not_required`/`unverified`/`prohibited`), `terms_checked_on`, `terms_url`, `terms_quote`, `terms_note`, `pollable`, `block_reason`, `evidence_age_days`, `evidence_stale`. No secrets |
| PUT | `/api/admin/legal-sources/{code}` | Body `{"enabled": bool}`. Refuses with 409 `terms_unverified`, `terms_prohibit_polling`, `no_documented_channel` or `region_disabled`; enabling a `feed` source also creates/updates its collector row (`legal-<code>`, T1, every 12 h) |

An API key cannot use these management endpoints. A valid management session cookie is required, and writes also require `Content-Type: application/json` and the CSRF header described above. Reader preferences stored in a browser override global display defaults on that browser. Catalog success responses use `{ "data": ... }`.

### Topic payload

```json
{
  "slug": "central-banks",
  "label_vi": "Ngân hàng trung ương",
  "label_en": "Central banks",
  "parent_slug": "finance",
  "enabled": true
}
```

The slug is 1–64 lowercase ASCII letters, digits or hyphens, without leading, trailing or consecutive hyphens. Labels are required and at most 120 characters. `parent_slug` is a topic slug or `null`. A topic slug is immutable after creation; the PUT path must match the payload slug. An enabled topic requires enabled ancestors. Cycles are rejected. Disable enabled child topics and sources first before disabling their parent. Disabled records remain in the management list; there is no delete endpoint.

### Source payload

```json
{
  "slug": "example-source",
  "name": "Example source",
  "url": "https://example.invalid/feed.xml",
  "tier": "T2",
  "enabled": false,
  "language": "en",
  "region": "fr",
  "topic_slug": "finance",
  "fetch_interval_minutes": 60
}
```

The slug follows the same format as topics; name is required and at most 120 characters. `tier` is `T1`, `T2`, `T3` or `T4`; `language` may be empty and `topic_slug` can be `null`. `region` is a region code; an empty value means `global`, and an unknown region or enabling a source in a disabled region returns 400 `invalid_region`. An enabled source cannot point to a disabled topic. The interval is an integer from 5 to 10,080 minutes. Responses also include `id`, `last_checked_at`, `last_success_at`, `last_error`, `consecutive_failures`, `paused_reason`, `next_fetch_at` (Unix seconds), `article_count` and `pack_key`; omit them when submitting a payload. `pack_key` non-null means the source is managed by a region pack: the PUT may only flip `enabled` (any other field change returns 409 `source_managed_by_pack`), and `slug` is namespaced `pack-<region>-<slug>`. The UI defaults new sources to disabled, but API clients must explicitly send `enabled`.

Stored URLs must use HTTP(S), have an authority, and contain no userinfo or fragment; obvious secret-like query parameter names are rejected. Do not store credentials in URLs. Adding or enabling a source makes no request by itself.

### Collection

Requests go only through the hardened fetcher: http/https on ports 80, 443, 8080 or 8443; every resolved address must pass a deny-list of known non-public ranges (not a complete allow-list) and the connection is pinned to the checked addresses; up to 5 redirects, each re-validated; no system proxy; 5 MB and 25 s limits. Conditional `If-None-Match` / `If-Modified-Since` are replayed only to the origin that issued them, and are cleared when the source URL changes.

The scheduler runs only while `collector_enabled` is true: every 30 s it picks up to 20 enabled, unpaused sources whose topic is enabled or empty, whose region is enabled (or an ancestor of an enabled region) and whose `next_fetch_at` has passed. Requests to one host are spaced by 5 s on **every** fetch path — scheduler, `Fetch now` and `Test feed` — through one shared gate. Success schedules the next run at the interval plus jitter. Failure doubles the wait up to 24 h; the 5th consecutive failure sets `paused_reason = "consecutive_failures"`. A run whose source URL, region or `enabled` flag changed while the request was in flight is discarded (`status: "discarded"`, `error: "source_changed"`) and stores nothing.

Stored per item: title (HTML stripped, ≤300 chars), canonical URL (fragment and tracking parameters removed), identifier, publication time in UTC, clean plain-text `summary` (≤420 chars) and extracted `content_text` (from RSS `<content:encoded>` or background/on-demand Readability web extraction, indexed in `articles_fts` with Vietnamese diacritics folded).

Fetch error codes: `invalid_url`, `blocked_address`, `dns_failed`, `timeout`, `network_error`, `response_too_large`, `too_many_redirects`, `http_<status>`, `not_a_feed`, `invalid_feed`.

Common catalog error codes include `invalid_topic`, `invalid_source`, `invalid_region`, `invalid_parent`, `topic_cycle` and `immutable_slug` (400); `duplicate_slug`, `duplicate_source`, `topic_in_use` and `source_managed_by_pack` (409); `not_found` (404); and `internal_error` (500).

### Region packs (P1.5)

The pack root **is** the virtual region `global`: it always exists and is always enabled, and it is not a sibling directory of `eu`/`us`/`vn`. Directories nested inside each other form the parent/child tree; a directory is a region only when it contains `region.toml`, and a region below a directory without one is reported as `region_orphan`. Region codes are folder names (lowercase ASCII, digits, hyphens) and must match `code` in `region.toml` when that key is present.

`region.toml`: `code` (optional), `name_vi`, `name_en`, `currency`, `timezone`, `language`. `sources.toml`: repeated `[[source]]` with `slug`, `name`, `url`, `tier`, optional `language`, `topic`, `interval_minutes`. Other files (`official.toml`, `tax.toml`, `markets.toml`, `macro.toml`) belong to P2–P4 and are ignored.

Scanning (at startup and on reload) discovers and registers; it never activates. Every pack region except `global` starts disabled, and every pack source starts disabled even when its region is on. Symlinked directories and paths resolving outside the pack root are skipped. Scan issue codes: `syntax_error`, `unreadable_file`, `unreadable_directory`, `pack_missing`, `code_mismatch`, `duplicate_region_code`, `invalid_region`, `invalid_source`, `duplicate_source`, `invalid_topic`, `region_orphan`, `region_conflict`, `region_cycle`, `symlink_skipped`, `path_escape`, `too_deep`, `too_many_issues`, `apply_failed`. A reload never deletes rows, so a broken pack keeps the previous valid configuration.

The built-in pack (`global`, `eu`, `eu/fr`, `us`, `vn`) is embedded in the binary and written to `REGIONS_DIR` only when that directory has no `region.toml`; existing files are never overwritten. Its sources point at the reserved `example.invalid` name and are placeholders, not vetted feeds. Authentication and CSRF failures use their own 401/403 responses. Duplicate source URL comparison is exact-string, not URL canonicalization.

## Session or API key

`Authorization: Bearer <key>` or `X-API-Key` is accepted for these read-only endpoints; `LNC_API_KEYS` is a comma-separated environment variable distinct from `MANAGEMENT_KEY`.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/status` | Service version, `phase: "P1-collector"` and `mcp_available: true` |
| GET | `/api/v1/topics` | Enabled topics only |
| GET | `/api/v1/regions` | Enabled regions only, plus the effective set: `{ data, effective }` |
| GET | `/api/v1/articles` | Newest first. Query: `topic` (includes descendant topics), `source` (one source slug, applied on top of the same visibility rules), `region` (that region plus its ancestors; 400 `invalid_region` if the region is unknown or disabled), `q` (folded FTS5 search across title, summary and full body), `limit` 1–100 (default 50), `offset` 0–10,000. Fields: `id`, `title`, `url`, `summary`, `content_source`, `word_count`, `published_at`, `fetched_at`, `topic_slug`, `region_code`, `source_name`, `source_tier`, `labels`. Items under disabled topics or outside the effective region set are hidden |
| GET | `/api/v1/articles/{id}` | Full visible article with `summary`, `content_text`, `content_source`, `word_count`, `source_attribution`, `labels` and attached `notes` |
| GET | `/api/v1/analyses` | Stored briefs and summaries in enabled regions, newest first. Query: `topic`, `region`, `updated_since` (ISO date, RFC 3339 or Unix seconds), `limit` 1–100 |
| GET | `/api/v1/analyses/{id}` | One stored brief with `stable_key` (`analysis-<id>`), resolved `cited_articles` and feedback `notes` |
| GET | `/api/v1/notes` | Notes on one target (`article_id`, `document_id` or `analysis_id`, `limit` 1–100), plus `flags` when querying an article |

## Alerts (P7 foundation)

Session only — an alert inbox is per person, so a read-only API key cannot read it.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/alerts` | Inbox, newest first. Query: `unread=1` to narrow, `limit` 1–200. Answers `{ data, unread }` |
| POST | `/api/alerts/{id}/read` | Marks one delivery read; answers `{"changed": boolean}` |
| POST | `/api/alerts/read-all` | Marks every unread delivery read; answers `{"changed": n}` |
| GET | `/api/admin/alerts/rules` | Every rule, enabled or not, plus the channels a rule may name right now: `in-app` always, `telegram` while `telegram_mode` is not `off` |
| POST | `/api/admin/alerts/rules` | Upserts a rule. A channel that is not implemented, or `telegram` while it is switched off, is refused with 400 `channel_not_implemented`. Declaring `telegram` does **not** require its secret to exist yet — a missing destination shows as `reason` on `/api/admin/telegram` and as queued rows |
| PUT | `/api/admin/alerts/rules/{slug}` | `{ "enabled": boolean }` |
| POST | `/api/admin/alerts/sweep` | Runs one sweep now; answers `{ created, queued_telegram, telegram_sent, telegram_failed, skipped }` |

A delivery is written at most once per (rule, article) — enforced by a UNIQUE row, not by sweep bookkeeping. Quiet-hour skips are logged and **do not** consume the event, so it fires after the window. Matching reuses the reader's enabled-region and enabled-topic predicates.

## Telegram channel (P7)

Two ways to send, chosen by `telegram_mode`: `relay` (the fleet gateway at `https://telegram.lengoc.me`, bot configured there; this app holds only a relay token) or `direct` (the canonical Bot API with our own bot token). `off` is the default. `TELEGRAM_MODE` in the environment overrides the stored setting; otherwise the one set in settings applies.

| Setting / secret | Meaning |
|---|---|
| `telegram_mode` | `relay` \| `direct` \| `off` (settings, or `TELEGRAM_MODE` env) |
| `telegram_chat_id` | Group id, negative for a supergroup (settings) |
| `telegram_alerts_topic_id` | Optional forum topic for alerts; empty sends to General |
| `TELEGRAM_RELAY_BASE_URL` | Relay base, default `https://telegram.lengoc.me`; https only |
| `TELEGRAM_ALLOWED_HOSTS` | Extra hosts allowed to resolve to a private address, comma-separated |
| secret `telegram-relay-token` | Relay API key, write-only via `/api/admin/secrets` |
| secret `telegram-bot-token` | Bot API token (direct mode), write-only |

Sending never runs while the database lock is held: the sweep queues one row per delivery in `alert_telegram_outbox` and a 30-second flusher claims it (`pending` → `sending`) before calling out, so a delivery can be sent at most once and a row stranded by a crash is reclaimed after 300 seconds. The message is the article title and its link — the same thing the in-app inbox shows.

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/admin/telegram` | Resolved mode, relay host check, `chat_id`, whether each secret is stored, `ready` + `reason`, and queue counts. Never returns a secret |
| POST | `/api/admin/telegram/flush` | Sends everything queued now; answers `{ ok, sent, failed, skipped }` (the same payload the background flusher uses) |

## MCP endpoint (P8)

`POST /mcp` speaks JSON-RPC 2.0. `GET /mcp` answers 405 `stream_not_supported`: this server does not open a server-sent event stream.

Three credentials are accepted, and they are told apart by **how** they arrive:

| Caller | Credential | Scope |
|---|---|---|
| Signed-in browser | session cookie (+ `X-Requested-With` on POST) | every tool |
| Reader client | `x-api-key` (or `Authorization: Bearer` with a reader key) | every tool |
| Business MCP Hub | `Authorization: Bearer <connector token>` plus the signed delegation headers below | only the scopes it was granted (`newsman:read`, optionally `newsman:write`) |

The Hub path refuses a call when the connector is not paired. Error codes: 401 `connector_auth_required` for a missing or wrong connector token (an unpaired connector answers identically — a caller cannot tell the two apart), 403 `delegation_denied` for a bad or missing signature, the wrong audience, a timestamp more than 90 s off, a replayed nonce, a malformed header, or a scope outside the connector ceiling.

Delegation headers, HMAC-SHA256 hex over the envelope, lowercase: `X-MCP-Hub-Client-ID`, `X-MCP-Hub-Client-Scopes` (normalised, sorted, comma-separated), `X-MCP-Hub-Client-Timestamp`, `X-MCP-Hub-Client-Nonce`, `X-MCP-Hub-Client-Signature`; optional `X-MCP-Hub-App-Id` switches the envelope to v4.

```text
v3
<audience>
<client_id>
<timestamp>
<nonce>
<scopes>
v4
<audience>
<client_id>
<app_id>
<timestamp>
<nonce>
<scopes>
```

Audience defaults to `lnc-newsman` (`CONNECTOR_AUDIENCE` or the admin card). A nonce is single-use for 300 s after a **valid** signature; an unauthenticated request cannot burn one. The app id is only ever the signed assertion, never a tool argument.

Methods: `initialize` (protocol `2025-06-18`, `serverInfo.name` = `lnc-newsman`), `ping`, `tools/list`, `tools/call`. A JSON-RPC batch is answered in order. An unknown method returns `-32601`. **An unknown tool and a tool the caller has no scope for return the same `-32601` with the same message** ("Unknown or unauthorized MCP tool"), and `tools/list` is filtered by the caller's scopes, so probing the namespace yields nothing. A bad tool argument is reported **inside** the result as `isError: true`, so a client can show the message without treating it as a transport failure.

Every tool answers through the same function the matching REST endpoint calls, so it can never expose a row REST hides. A tool that needs provider or phase data that does not exist yet is **not listed** — a missing tool is better than one answering "no data" for months.

**Tool names carry no namespace.** The Business MCP Hub prepends its own, so an agent reaches these as `newsman__regions`; advertising `newsman__regions` upstream would produce `newsman__newsman__regions`. A client written against v0.4.3 that still sends `newsman__<tool>` is accepted, because the prefix is stripped before lookup.

### Read tools — scope `newsman:read`

| Tool | Arguments | Same data as |
|---|---|---|
| `regions` | — | `GET /api/v1/regions` |
| `topics` | — | `GET /api/v1/topics` |
| `reader_preferences` | — | Non-sensitive `focus_profile`, `language`, `brief_morning_time`, `brief_evening_time`, `brief_timezone`, `effective_regions` and `enabled_topics` |
| `latest_articles` | `region?`, `topic?`, `q?`, `limit?` 1–100 | `GET /api/v1/articles` |
| `article_get` | `id` (required) | `GET /api/v1/articles/{id}` |
| `documents` | `region?`, `topic?`, `q?`, `limit?` 1–100 | `GET /api/v1/documents` |
| `tax_calendar` | `region?`, `horizon_days?` 1–366 | `GET /api/v1/tax-calendar` |
| `series` | `region?`, `limit?` 1–500 | `GET /api/v1/series` |
| `observations` | `code` (required), `limit?` 1–200 | `GET /api/v1/series/{code}/observations` |
| `notes` | exactly one of `article_id` / `document_id` / `analysis_id`, `limit?` 1–100 | — (`GET /api/v1/articles` then the note list) |
| `analyses` | `topic?`, `region?`, `updated_since?` (ISO date or Unix seconds; strictly after), `limit?` 1–100 | — |
| `write_log` | `limit?` 1–200 | — |

### Write tools — scope `newsman:write` (also needs `newsman:read`)

These write **only** the app's own analytic data. None of them can reach a source, a region, a connector, a secret, the collector switch or alert delivery — that stays behind `/admin` and a signed-in owner session.

| Tool | Arguments | Effect |
|---|---|---|
| `note_create` | `body` (≤4000), exactly one of `article_id` / `document_id` / `analysis_id` | Adds a note; the author is the delegated client id. A note on an analysis is **feedback**: it never modifies the analysis |
| `note_update` | `id`, `body` | Replaces the text of a note **you wrote** |
| `note_delete` | `id` | Deletes a note **you wrote** |
| `analysis_create` | `title` (≤300), `body` (≤20000), `kind?` (`brief`/`summary`), `topic_slug?`, `region_code?`, `source_article_ids?` (≤200) | Stores a brief or summary |
| `analysis_update` | `id`, any of `title` / `body` / `source_article_ids` | Changes **your own** brief |
| `article_flag` | `article_id`, `label` (≤60) | Marks an article; repeating a label is idempotent |
| `brief_propose` | `title` (≤300), `body` (≤20000), `kind?` (`brief`/`summary`), `topic_slug?`, `region_code?`, `source_article_ids?` (≤200) | Queues a gated brief publication proposal (`status: "pending_approval"`) with `approval_required: { action_type: "newsman.brief.publish", summary, request_ref }` instead of publishing directly |

Rules that hold for every write tool:

- **A note may target an article, a document or an analysis**, chosen by exactly one of three ids (migration 19 rebuilt `newsman_notes` so the `CHECK` allows all three; SQLite cannot alter a constraint, so the table was copied and renamed). A note on an analysis is feedback: the analysis is never modified, updated or deleted through the connector.
- **`analyses` carries `stable_key`** — `analysis-<id>`, derived from the immutable row id, so a caller can name a file after it and keep that name across edits. `updated_since` makes the pull incremental: an ISO date (`2026-11-12`, meaning the start of that day in UTC) or Unix seconds, always **strictly after**. An unusable value is an error, never a filter that quietly returns everything.
- **An undeclared argument is refused on every tool, read and write.** Silently ignoring one is worse than an error: the caller believes it applied a filter (or set an author) that never took effect. The check derives the allowed names from the tool's own schema, so adding a parameter cannot drift out of sync — a test walks every tool and proves a declared argument is never refused.
- **The author is the verified delegation**, never an argument. No tool has an `author` field, and an **undeclared argument is refused** (`isError` with `unknown argument: author`) rather than ignored — `additionalProperties: false` alone is a note to the client, so the server enforces it. This is what stops an agent believing it set an author it did not.
- **Ownership is enforced on update and delete.** A note or brief written by the owner in the web UI has no author and is not reachable through this boundary; one written by another client is refused too.
- **Every write is appended to `newsman_write_log`** (time, client id, app id, tool, record kind, record id) and is readable through `write_log`. No tool can insert into or alter that table except the write path itself.
- **Every id is checked against what the app already shows.** A note needs an existing article or document; a cited article id must be stored; a brief cannot name a disabled region or topic.
- **Article text is data, not instruction.** Text stored from a feed is quoted into a summary and never interpreted; the `initialize` instructions say so to the model.
- Lengths and counts are bounded (`too_long`, `too_many_sources`, `invalid_sources`), and a malformed citation list is refused rather than silently truncated.

**An unknown tool and a tool the caller has no scope for answer identically** (`-32601`, "Unknown or unauthorized MCP tool"), and `tools/list` is filtered by the caller's scopes, so probing the namespace yields nothing.

Still to come with their phases: `market_snapshot` (needs live provider data) and `region_brief` (P6).

## Secrets and connectors (P2 foundation)

`LNC_MASTER_KEY` is 64 hex characters or a 32-byte base64 value; `LNC_MASTER_KEY_OLD` holds the previous key during rotation. Values are sealed with XChaCha20-Poly1305, a fresh 24-byte nonce per write, and the secret name as associated data. Without the key the store is closed (503) rather than falling back to plaintext. Secret names are lowercase slugs of at most 64 characters, values up to 8 KiB.

Six provider connectors ship, all **disabled by default** and none accepted against a live provider yet: `mempool-space`, `ecb-sdmx`, `federal-register`, `fred` (needs `fred-api-key`), `coingecko` (needs `coingecko-api-key`, sent as a header) and `sec-edgar` (needs `SEC_USER_AGENT` with a contact address). The framework (`src/connectors.rs`) gates every run on: connector enabled, region enabled, required operator configuration present, secret present and decryptable, UTC-daily quota, cache window, then schedule. The quota is charged per **HTTP request** (`requests_last_run`), not per run. Each attempt updates `health`/`last_error`. A connector receives the hardened fetcher from the framework and cannot create its own HTTP client; a redirect to another origin does not carry an authentication header.

Every displayed value carries the provider's required credit line in `attribution` (for example `Source: ECB statistics`, `Source: CoinGecko (https://www.coingecko.com)`). Values are stored verbatim: no rounding, no unit conversion, and a missing provider value is skipped rather than filled. See `.docs/live-acceptance.md` for the separately approved live run.

## Series and observations (P2 foundation)

`GET /api/v1/series?region=` lists enabled series whose region is in the effective set, each with its `attribution`; `GET /api/v1/series/{code}/observations?as_of=&limit=` returns one row per period — the revision known at `as_of` (or the latest). `GET /api/v1/events?from=&to=` lists scheduled releases. A correction is stored as a new revision, so an `as_of` query can reproduce what was known then. No provider is connected: these endpoints are empty until a real connector is approved and run.

## Official documents (P2)

`GET /api/v1/documents?region=&limit=&q=&topic=` lists documents in the effective region set. Stored fields are the provider, region, type, agency, official number, title, effective date, publication date, original URL, a topic and a summary **we** write — never the publisher's abstract. A repeated refresh updates metadata but never overwrites a hand-written summary. Federal Register documents use `effective_date` as the request filter and `effective_on` as the response field.

`q` searches title, agency and summary through an FTS5 index whose stored copy has Vietnamese diacritics folded, so `thue` matches `thuế`, and each token is quoted (a query cannot inject FTS operators). `topic` filters on the stored topic slug (`tax` today). Search hits are re-filtered by the same region and topic rules as a plain listing, so a hit in a disabled region never leaks.

Any document that carries a summary also carries `disclaimer`, always the server's own constant (`Không phải tư vấn pháp lý/thuế./Not legal or tax advice.`); a client cannot supply or remove it. A document without a summary has an empty disclaimer.

## Tax calendar (P3)

`GET /api/v1/tax-calendar?region=&horizon_days=` returns the next occurrence of every **enabled** deadline declared by a region pack for the effective region set, sorted by due date, together with `disclaimer_vi`, `disclaimer_en` and `holidays_modelled: false`.

Each entry has `code`, `region`, `cadence` (`monthly`/`quarterly`/`yearly`), `due_on`, `nominal_on` (before any weekend shift), `days_until` (0 = today), `due_soon` (within the rule's `lead_days`), `weekend_rule`, `verified` and the rule's own `note`. Dates are computed in the region's `timezone` from `region.toml`; an unknown zone falls back to UTC. The weekend rule is exactly what the pack declares (`next_business_day`, `previous_business_day`, `none`); **public holidays are not modelled**, and a day in a short month is clamped to that month's last day. A rule with `verified = false` has not been checked with the authority and the reader shows that notice.

Admin: `GET /api/admin/tax-rules` lists every declared rule including disabled ones, and `PUT /api/admin/tax-rules/{code}` with `{ "enabled": boolean }` toggles one. Enabling is refused with 409 `invalid_region` while the rule's region is off. No reminder is delivered: P3 only shows the calendar in the app; Telegram/email belong to P7.

## Classification and the LLM layer (P5)

Rule labels live in `article_matches` (`kind` = `region`/`topic`/`entity`, `origin` = `rule`/`llm`/`human`). They are produced during collection with no network call. `POST /api/admin/classify` takes `{ "title": "...", "region": "global" }` and returns the rule labels, plus an `llm` block when the layer is on; `mode` says which ran (`rules`, `rules+llm`, `rules_llm_unavailable`). `POST /api/admin/classify/reclassify` re-runs the rules over every stored article.

The LLM layer is **off by default**. `GET /api/admin/llm` returns `configured`, `enabled`, `model`, `tokens_used_today`, `daily_budget`, `calls_today` and `has_secret` — never the key or the provider URL. `PATCH /api/admin/llm` with `{ "enabled": boolean }` toggles it and returns 409 `llm_not_configured` when `LLM_BASE_URL` is unset. Provider settings come from `LLM_BASE_URL` and `LLM_MODEL`; the key is the `llm-api-key` secret. A request is refused before it is sent when the UTC-daily token budget is spent, answers are cached by hash of model + prompt version + title, and any provider failure leaves the rule labels in place.

## Company filings and financial facts (P4 foundation)

`GET /api/v1/companies?region=&limit=` lists companies (name, ticker, exchange, region) in the effective region set. `GET /api/v1/companies/{id}/filings?form=` lists filings for a company in an enabled region: form type, accession, period end, filed date, `filing_kind` (`annual`/`quarterly`/`current`/`registration`) and the original link. `GET /api/v1/companies/{id}/facts?metric=&limit=` returns the latest revision per metric and period: `metric`, provider `concept`, period, unit, currency, `as_of`, `revision`, `quality`. Responses carry `disclaimer` (`Không phải tư vấn đầu tư…`).

Only metadata and structured numbers are stored; filing bodies are never fetched. A restatement is a new revision, not an overwrite. No ratios are computed by this API.

## Hub provider (P5/P6)

`GET /api/admin/provider` returns `configured`, `base_host` (host only), `allowed_hosts`, the three model aliases, `agent`, `timeout_seconds`, `tokens_used_today`, `daily_budget`, `calls_today` and `has_secret` — never the key and never the full base URL. `POST /api/admin/provider/check` performs a single `GET {HUB_BASE_URL}/models` with the Hub key as a bearer header and returns the model ids; it generates no content and bills no tokens.

Configuration is environment-only: `HUB_BASE_URL`, `HUB_ALLOWED_HOSTS` (comma-separated exact host names that may resolve to private addresses), `HUB_MODEL_CLASSIFY`, `HUB_MODEL_SUMMARISE`, `HUB_MODEL_ANALYSE`, `HUB_AGENT_ID`, `HUB_TIMEOUT_SECONDS`, `LLM_DAILY_TOKEN_BUDGET`. The Hub key is the write-only secret `hub-api-key`. See `.docs/hub-integration.md`.

## App-scoped agents (Studio, Editorial Rooms, Reverse Relay & Telegram)

App-scoped newsroom agents (`macro-argus`, `tax-linh`, `editor-minh`, plus owner-created blueprints) operate under zero-trust `v4s` capability envelopes (`audience == "lnc-newsman"`, `app_id == "lnc-newsman"`, 90 s freshness, single-use nonce, and canonical SHA-256 `ResourceCtx { regions, topics }` binding) over both the outbound reverse relay (`lnc-app-bridge.v1`) and HTTP `POST /mcp` (`X-MCP-Hub-Version: v4s` or `X-LNC-V4S-Grant`). When Astraea is offline or unconfigured, turns execute locally in `hub_standby` mode with the 4-mode discussion orchestrator (`parallel_panel`, `sequential_build`, `cross_debate`, `mention_or_lead` capped at 3 peer hops) and a 512-event SSE ring buffer emitting `seat_state`, `text`, `reasoning`, `tool`, `tool_result`, `result`, and `error`. All secrets are sealed exclusively via `LNC_MASTER_KEY` (`src/secrets.rs`), and all outbound HTTP and WebSocket connections pass through `fetch::Fetcher`.

| Endpoint | Auth & CSRF | Purpose |
|---|---|---|
| `GET /api/app-agents/bridge-config` | Session | Read non-secret bridge settings, `mode` (`astraea_relay` or `hub_standby`), `relay_connected`, `relay_route` (`hub` / `direct` / `disabled`), `routing` (5-leg Star Topology matrix), `astraea_reachable`, `secret_store_open`, `hub_bundle`, `capability_bundles`, `mcp_groups`, `allowed_mcp_tools`, `allowed_model_aliases`, `channel_grants`, `task_models` (`thread_title`, `round_summary`, `companion_brief`, `tax_doc_digest`), and redacted `has_*` flags |
| `PUT /api/app-agents/bridge-config` | Session + CSRF | Update Hub-First URL/token and optional direct override URLs, default model, `task_models`, allowed private hosts, and write-only secrets (returns 503 `secret_store_not_configured` if `LNC_MASTER_KEY` is unset when writing a secret); triggers background Hub sync when configured |
| `POST /api/app-agents/bridge-config/probe` | Session + CSRF | Probe Hub, Astraea (or Hub `/effective-bundle` `via_hub`), and Model Fabric `/models` endpoint via `fetch::Fetcher` (never calls chat completions), returning `{ ok, probed_at, hub, astraea, llm, routing, relay_route }` without echoing secrets |
| `GET /api/app-agents/models` | Session | List available model aliases from Model Fabric (`llm_base_url` override or `{hub_root}/v1/models` with `hub_token`) or standby defaults |
| `GET /api/app-agents/workspaces` | Session | List newsroom workspaces (seeded `newsroom-general` and `vn-macro-tax-desk` plus custom workspaces) |
| `POST /api/app-agents/workspaces` | Session + CSRF | Create a workspace (`custom_instructions_md`, `resource_ctx`, `default_skill_slugs`, `default_agent_slugs`, `default_discussion_mode`, `is_default`) |
| `PUT /api/app-agents/workspaces/{id}` | Session + CSRF | Create or update a workspace |
| `DELETE /api/app-agents/workspaces/{id}` | Session + CSRF | Delete a workspace (unlinks `workspace_id` on existing threads) |
| `GET /api/app-agents/skills` | Session | List cached Hub/builtin skills (`display_name`, `granted`, local `enabled`, `group_slugs`, `body_md`, safe `body_preview`), merged `groups`, `access_requests`, and cached Hub bundle (`capability_bundles`, `mcp_groups`, `allowed_mcp_tools`, `allowed_model_aliases`, `channel_grants`, `astraea_bridge_ready`, `hub_bundle`) |
| `POST /api/app-agents/skills/sync` | Session + CSRF | Push `sync_blueprint_to_hub` agents via `/agents/sync` and sync `/effective-bundle` (fallback `/skills` -> `/v1/skills`) plus `/access-requests` (fallback `/skill-access-requests`) via `fetch::Fetcher`, preserving local `enabled`, clearing `body_md` when revoked, and returning `sync_mode`, `bundle_synced`, `agents_synced`, `capability_bundles`, `mcp_groups`, `hub_bundle`, and `access_requests`; degrades cleanly when offline |
| `GET /api/app-agents/skills/{slug}` | Session | Fetch a single cached skill playbook |
| `PATCH /api/app-agents/skills/{slug}` | Session + CSRF | Toggle local `enabled` state only (never elevates Hub `granted`) |
| `GET /api/app-agents/skills/access-requests` | Session | Reconcile access requests from Hub `/access-requests` (fallback `/skill-access-requests`) when online and list local rows (`status`, `hub_request_id`, `approval_request_id`, `reviewed_by`, `review_note`) |
| `POST /api/app-agents/skills/access-requests` | Session + CSRF | Idempotently submit/upsert a Hub access request (`target_kind`: `capability_bundle`, `mcp_group`, `skill_group`, `skill`, `model_alias`, or `channel`) to `/access-requests` (404/405 fallback `/skill-access-requests`), returning `"pending"` + `approval_request_id` or `"recorded_local"` when offline |
| `GET /api/app-agents/agents` | Session | List newsroom agent blueprints (`AppAgentConfig`, including `enabled_skills` and `enabled_skill_groups`) |
| `PUT /api/app-agents/agents/{slug}` | Session + CSRF | Create or update an agent blueprint (`vault://` secret references only; optional Hub binding sync) |
| `DELETE /api/app-agents/agents/{slug}` | Session + CSRF | Delete an agent blueprint |
| `GET /api/app-agents/threads` | Session | List direct and group editorial threads (`?article_id=&report_id=&workspace_id=`) |
| `POST /api/app-agents/threads` | Session + CSRF | Create a direct or group thread with `discussion_mode`, `member_slugs`, optional `workspace_id`, `pinned_skills`, and `model_override` |
| `PATCH /api/app-agents/threads/{id}` | Session + CSRF | Update thread title, members, lead, `discussion_mode`, `workspace_id`, `pinned_skills`, or `model_override` |
| `DELETE /api/app-agents/threads/{id}` | Session + CSRF | Delete a thread and cascade its messages and whiteboard notes |
| `GET /api/app-agents/threads/{id}/messages` | Session | List persisted thread messages, reasoning, merged tool pills, and context snapshots |
| `POST /api/app-agents/threads/{id}/turns` | Session + CSRF | Execute a direct or multi-agent turn (`text`, `idempotency_key`, `mentions`, optional `skill_slugs`, `model_override`, and 16 KiB-bounded `active_view_context`) |
| `GET /api/app-agents/threads/{id}/stream` | Session | Subscribe to SSE turn events with `Last-Event-ID` / `?since_seq=` replay |
| `GET /api/app-agents/threads/{id}/whiteboard` | Session | List pinned whiteboard notes for a thread |
| `POST /api/app-agents/threads/{id}/whiteboard` | Session + CSRF | Pin a note onto the thread whiteboard |
| `DELETE /api/app-agents/threads/{id}/whiteboard` | Session + CSRF | Delete a whiteboard note by `?id=` query parameter |
| `DELETE /api/app-agents/threads/{id}/whiteboard/{code}` | Session + CSRF | Delete a whiteboard note by path id |
| `POST /api/app-agents/threads/{id}/round-summary` | Session + CSRF | Synthesize the latest discussion round onto the whiteboard |
| `GET /api/app-agents/schedules` | Session | List recurring interval or 5-field cron agent schedules |
| `POST /api/app-agents/schedules` | Session + CSRF | Create a recurring agent schedule |
| `PATCH /api/app-agents/schedules/{id}` | Session + CSRF | Update or toggle a schedule |
| `DELETE /api/app-agents/schedules/{id}` | Session + CSRF | Delete a schedule |
| `POST /api/app-agents/schedules/{id}/run-now` | Session + CSRF | Trigger an immediate schedule turn |
| `GET /api/app-agents/telegram/config` | Session | Read conversational Telegram transport (`direct_poll`, `direct_webhook`, `cygnus_poll`, `cygnus_webhook`), `telegram_route` (`hub_cygnus`, `direct_cygnus`, `direct_bot_api`, `disabled`), `chat_policy` (`open`, `mention`, `topics_only`), and `has_*` flags |
| `PUT /api/app-agents/telegram/config` | Session + CSRF | Update conversational Telegram transport, policy, URLs, and write-only secrets |
| `GET /api/app-agents/telegram/routes` | Session | List conversational Telegram chat/topic-to-agent bindings |
| `POST /api/app-agents/telegram/routes` | Session + CSRF | Create a conversational Telegram route (`chat_id`, optional `message_thread_id`, `agent_slug`, `policy`) |
| `DELETE /api/app-agents/telegram/routes/{id}` | Session + CSRF | Delete a conversational Telegram route |
| `POST /api/app-agents/telegram/webhook/direct` | `X-Telegram-Bot-Api-Secret-Token` | Receive inbound Telegram Bot API webhook updates (constant-time secret check; separate from `src/telegram.rs` alert outbox) |
| `POST /api/app-agents/telegram/webhook/cygnus` | `x-gateway-key` | Receive inbound Cygnus gateway webhook envelopes (constant-time key check; preserves `message_thread_id`) |

## Security and deployment limitations

`MANAGEMENT_KEY` is currently **environment-managed**, not a bootstrap-to-runtime store. Changing it requires restart and invalidates old cookie signatures. The server stores only hashes of signed random session tokens, with expiry and logout revocation in SQLite. `COOKIE_SECURE` defaults to `true`; set it to `false` only for isolated HTTP testing. Login throttling is process-local and keyed by the TCP peer by default. When the request arrives from a socket peer listed in
`TRUSTED_PROXIES` (comma-separated IPs, e.g. the Caddy edge `10.20.0.3`), the key becomes the right-most valid address in
`X-Forwarded-For` — the one that proxy appended. **Measured 2026-11-12:** in the deployed stack the socket peer is the Docker
bridge gateway (`172.31.0.1`) for both the domain and the published port, so a `TRUSTED_PROXIES` naming the edge never matches
and all callers share one bucket. See `.docs/PROJECT_STATUS.md` for the measurement and the options; nothing was changed. Earlier entries are caller-supplied and are ignored, and a malformed value
falls back to the proxy's own address. Empty `TRUSTED_PROXIES` trusts nobody, which means a shared reverse proxy makes every
user share one limit. Multi-replica coordination, management key rotation UI, encrypted provider secrets, audit chain and independent reader accounts remain unimplemented. The application is not production-ready.

## Planned only

Stories, reports, timeline, in-app search across items, the Hub registration of `/mcp` (the endpoint itself exists), the pack files for P2–P4 that have no loader yet, and every live data connector are not implemented. Do not configure clients against those paths yet.

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
| GET | `/api/settings` | Global language and reader-mode defaults |
| PATCH | `/api/settings` | Partial JSON: `language` = `vi`/`en`, `default_reader_mode` = `paper`/`sepia`/`night`, `collector_enabled` = boolean (default `false`) |
| GET | `/api/admin/topics` | List enabled and disabled topics |
| POST | `/api/admin/topics` | Create a topic; returns 201 |
| PUT | `/api/admin/topics/{slug}` | Replace topic fields |
| GET | `/api/admin/sources` | List enabled and disabled sources |
| POST | `/api/admin/sources` | Create a source; returns 201 |
| PUT | `/api/admin/sources/{id}` | Replace source fields by numeric ID |
| POST | `/api/admin/sources/preview` | `{ "url": "..." }`: fetch and parse once, store nothing; returns `items_seen` and up to 5 sample titles. Failures return 502 `{ "error": "preview_failed", "reason": "..." }` |
| POST | `/api/admin/sources/{id}/fetch` | Fetch one source now (also works for a disabled source); returns `status` (`ok`/`not_modified`/`error`), `items_seen`, `items_new`, `error`, `paused`. 409 `fetch_in_progress` if already running |
| POST | `/api/admin/sources/{id}/resume` | Clear an automatic pause and failure count; the source becomes due immediately |
| GET | `/api/admin/sources/{id}/runs` | Last 20 runs (start time, duration, status, counts, error code) |
| GET | `/api/admin/regions` | Full region tree (enabled and disabled), the effective set and the last pack scan: `{ data, effective, pack: { last_loaded_at, issues, regions_registered, sources_registered } }` |
| PUT | `/api/admin/regions/{code}` | Body `{"enabled": boolean}`. Enabling needs an enabled parent; disabling needs every region below it (any depth) to be off; `global` cannot be disabled (`region_required`). Errors: 409 `invalid_parent_region`, `region_in_use`, `region_required`; 404 `not_found` |
| POST | `/api/admin/regions/reload` | Re-scan the pack directory (`REGIONS_DIR`, default `$DATA_DIR/regions`). Registers new regions and sources **disabled**; keeps previous valid rows when a file is broken. Returns the same `pack` object with per-file `issues` (`{ path, code, detail }`) |
| GET | `/api/articles` | Same as `/api/v1/articles` for the reader UI |
| GET | `/api/admin/secrets` | Secret names, key version and `updated_at` only, plus `key_fingerprint`. **Never returns a value.** 503 `secret_store_not_configured` when `LNC_MASTER_KEY` is absent |
| PUT | `/api/admin/secrets/{name}` | `{ "value": "..." }` — write-only; the value is encrypted immediately and cannot be read back |
| DELETE | `/api/admin/secrets/{name}` | Remove a secret |
| POST | `/api/admin/secrets/rewrap` | Re-encrypt every secret under the current key with a new nonce (rotation). Returns `rewrapped`; 409 `secret_rewrap_failed` and **no changes** if any row cannot be opened |
| GET | `/api/admin/connectors` | Connector id, provider, region, enabled, health, `last_error`, run times, `requests_today`/`daily_quota`, `requests_last_run`, `needs_secret`. No secrets |
| POST | `/api/admin/connectors/{id}/run` | Run one connector now. Same gates as a scheduled run: 409 with `connector_disabled`, `connector_region_off`, `connector_secret_missing`, `connector_not_configured`, `connector_quota_exhausted`, `connector_cache_fresh` or `connector_not_due`; 502 with `connector_fetch_failed`, `connector_provider_error`, `connector_unauthorized`, `connector_invalid_data` |

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

Stored per item: title (HTML stripped, ≤300 chars), canonical URL (fragment and tracking parameters removed), identifier and publication time in UTC. Descriptions and article text are never stored or returned.

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
| GET | `/api/status` | Service version, `phase: "P1-collector"` and `mcp_available: false` |
| GET | `/api/v1/topics` | Enabled topics only |
| GET | `/api/v1/regions` | Enabled regions only, plus the effective set: `{ data, effective }` |
| GET | `/api/v1/articles` | Newest first. Query: `topic` (includes descendant topics), `region` (that region plus its ancestors; 400 `invalid_region` if the region is unknown or disabled), `limit` 1–100 (default 50), `offset` 0–10,000. Fields: `id`, `title`, `url`, `published_at`, `fetched_at`, `topic_slug`, `region_code`, `source_name`, `source_tier`. Items under disabled topics or outside the effective region set are hidden |

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

## Security and deployment limitations

`MANAGEMENT_KEY` is currently **environment-managed**, not a bootstrap-to-runtime store. Changing it requires restart and invalidates old cookie signatures. The server stores only hashes of signed random session tokens, with expiry and logout revocation in SQLite. `COOKIE_SECURE` defaults to `true`; set it to `false` only for isolated HTTP testing. Login throttling is process-local and keyed by the TCP peer by default. When the request arrives from a socket peer listed in
`TRUSTED_PROXIES` (comma-separated IPs, e.g. the Caddy edge `10.20.0.3`), the key becomes the right-most valid address in
`X-Forwarded-For` — the one that proxy appended. Earlier entries are caller-supplied and are ignored, and a malformed value
falls back to the proxy's own address. Empty `TRUSTED_PROXIES` trusts nobody, which means a shared reverse proxy makes every
user share one limit. Multi-replica coordination, management key rotation UI, encrypted provider secrets, audit chain and independent reader accounts remain unimplemented. The application is not production-ready.

## Planned only

Stories, reports, alerts, timeline, search, LLM/Agent Hub integration, `/mcp`, the pack files for P2–P4 and every data connector are not implemented. Do not configure clients against those paths yet.

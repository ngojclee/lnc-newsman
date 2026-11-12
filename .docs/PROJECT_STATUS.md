# LNC Newsman — Project Status

Updated: 2026-11-12

## Current milestone

**P3 foundation is implemented offline (official/tax pack files, documents search, tax-calendar engine, Regions page); the deadline data is sample data marked `verified = false` and nothing is accepted.** P2 now includes six provider connectors (mempool.space, ECB, Federal Register, FRED, CoinGecko, SEC EDGAR), all tested against loopback mocks only. None has been run against a live provider and none is accepted.** The live acceptance script is `.docs/live-acceptance.md` and it has **not been run**. P1.5 (region tree + region packs) was approved in review; its real-source acceptance is still open. P1 collector acceptance (≥20 real RSS sources) and P0 container acceptance remain open. No live feed has been fetched, nothing is deployed, and no source in the built-in pack is a vetted feed.

## Scope update (plan v0.2, 2026-11-11)

Scope widened to a finance/Bitcoin on-chain/stock-market/official-and-tax information hub for regions France (with EU as parent), USA and Vietnam, with regions added by dropping a folder into the region pack directory. Phases are P0–P9 in [plan.md](plan.md); P1.5 is now implemented, P2 onward is not.

Plan clarification made before coding P1.5: **`global` is a virtual root**, not a sibling directory. The pack root *is* `global`; it always exists, is always enabled and cannot be disabled or renamed. Nested directories form the parent/child tree, so `asia/sg/` makes `sg` a child of `asia`.

## Implemented

- Rust/Axum service with SQLite migrations (versions 3, 4 and 5 tracked in `schema_migrations`) and five sample topics.
- Management session (signed, revocable) and a separate read-only API key.
- React/Vite reader with VI/EN, Paper/Sepia/Night. Custom accessible dropdown (rounded menu matching the trigger, keyboard support, portal so cards cannot clip it). Serif stack switched from Georgia, which lacks precomposed Vietnamese glyphs, to fonts with full Vietnamese coverage.
- Source and topic catalog (P1 catalog) with dependency, cycle and duplicate checks.
- **Collector**: RSS 2.0, RSS 1.0/RDF and Atom parsing; stores only title, canonical link (tracking parameters removed), identifier and publication time — never descriptions or article text. Deduplication by canonical URL and by source identifier. ETag/Last-Modified conditional requests.
- **Outbound safety**: http/https only on ports 80/443/8080/8443, no credentials; every resolved address must pass a **deny-list** of known non-public ranges (loopback, private, link-local, CGNAT/NetBird, multicast, documentation, benchmarking, 6to4 relay, reserved, IPv4-mapped) — this is not a complete allow-list, so a public address that happens to belong to the operator's own internal network is still reachable; the connection is pinned to the resolved addresses; redirects (max 5) are re-validated hop by hop; system proxies ignored; 5 MB body cap; 25 s timeout. ETag/Last-Modified are only replayed to the origin that issued them and are cleared when a source URL changes.
- **Scheduler**: global switch `collector_enabled`, **off by default**. Checks every 30 s, up to 20 due sources per pass, at least 5 s between requests to the same host, per-source interval with jitter. Failures back off exponentially (capped at 24 h); five consecutive failures pause the source until an admin resumes it. Last 50 runs per source are kept. Since P1.5 a source also runs only when its region is enabled (see below).
- Admin actions: test a feed before saving (nothing stored), fetch now, resume, run history. Source list shows last success, last error and item count.
- Reader: Today lists latest items; Topics filters by a topic including its descendants, with counts. Every item links to the original in a new tab.
- `GET /api/v1/articles` for API-key consumers (metadata only).

### P1.5 — regions and region packs

- Migration 5 adds `regions` (code, parent, VI/EN labels, currency, timezone, language, enabled, builtin, managed), reuses `sources.region` as the region code, adds `sources.pack_key` and `articles.region_code`. Existing sources and articles are preserved and adopted into `global`; nothing is deleted. Because P1 accepted free text in `sources.region`, any legacy value that is not a known region code is reset to `global` — otherwise those sources' items would silently leave the reader. Each pack load repeats that fallback for hand-edited rows and re-points articles at their source's region.
- Pack directory: `REGIONS_DIR` (default `<DATA_DIR>/regions`). The built-in pack (`global`, `eu`, `eu/fr`, `us`, `vn`) is embedded in the binary and written out only when the directory has no `region.toml`; existing files are never overwritten. Built-in sources point at the reserved `example.invalid` name and are placeholders.
- Scanning runs at startup and on **Reload regions**. It only registers: every pack region except `global`, and every pack source, starts **disabled**. Each file is validated (TOML syntax, unknown keys, code/folder mismatch, duplicates, missing parent, orphan beneath a folder without `region.toml`, cycles, invalid region fields, invalid source URL/tier/interval, topic that does not exist, clash with a hand-added source URL). A rejected file or folder produces an issue shown in the UI; other regions still load and previously valid rows are kept, because a reload never deletes rows. Symlinked directories and paths resolving outside the pack root are skipped.
- Effective region set = enabled regions plus all ancestors. Enabling a region requires an enabled parent; disabling one requires everything below it to be off; `global` cannot be disabled.
- The scheduler, `/api/articles`, `/api/v1/articles` and `/api/dashboard` (all with a `region` parameter) use that set, and the dashboard counters follow the same set so the numbers always match the list; `/api/v1/regions` lists enabled regions. Choosing France returns EU and global items.
- Pack-managed sources can only be enabled or disabled through the API/UI; any other field change returns 409 `source_managed_by_pack`. Hand-added sources pick a region like before, and enabling one in a disabled region is rejected.
- UI: region picker in the header (existing `Select` component), region tree with switches, reload button and pack-issue list in Settings; the source table marks pack-managed rows, shows each source's region, and warns when a source is enabled while its region is off (that source is skipped by the scheduler).

### P2 foundation (secrets, series, connectors)

- **Secret store** (`src/secrets.rs`, migration 7): XChaCha20-Poly1305 under `LNC_MASTER_KEY` (64 hex or 32-byte base64), a fresh 24-byte nonce on every write, the secret name bound as associated data. `LNC_MASTER_KEY_OLD` allows reads during rotation; `POST /api/admin/secrets/rewrap` decrypts everything first and writes only if every row opens, so a wrong key never destroys data. Without the key the store is closed (503), never plaintext. The admin API returns names, key version, `updated_at` and a short fingerprint only; there is no read-back endpoint, and `SecretBox`'s `Debug` is redacted.
- **Series store** (`src/series.rs`, migration 8): `series`, `observations` (`observed_at` = period, `as_of` = when it became known, `revision`, value, quality, source), `series_events`, `connectors`. A correction is a new revision; an `as_of` query returns what was known then; re-polling the same value is a no-op.
- **Connector framework** (`src/connectors.rs`): the `Connector` trait receives the system `fetch::Fetcher`, so every request keeps the SSRF guard, pinned DNS, per-hop redirect validation and size/time limits. The framework — not the connector — enforces enabled, region enabled, secret present and decryptable, UTC-daily quota, cache window and schedule, and records quota use, health and `last_error` on every attempt.
- **Read API and UI**: `/api/v1/series`, `/api/v1/series/{code}/observations?as_of=`, `/api/v1/events` (enabled regions only); reader page "Thị trường" with an honest empty state; admin blocks "Provider keys" (write-only) and "Connectors".
- **Provider connectors** (`src/providers/`), registered disabled at startup, each with its own loopback tests: `mempool-space` (4 requests/run, 10 min interval, 144/day), `ecb-sdmx` (1 request/run, 6 h, 12/day, credit `Source: ECB statistics`), `federal-register` (≤2 pages/run, 6 h, 6/day, refuses a window reporting >10,000 hits), `fred` (key from the secret store, values verbatim, `"."` skipped), `coingecko` (key sent as a header, ≤2 requests/minute, 30/day, stops at 450/month, mandatory attribution) and `sec-edgar` (refuses to run without an owner-supplied `SEC_USER_AGENT` containing a contact address; ≤60 requests/minute against the published 10/second ceiling). The daily quota is charged per HTTP request, not per run. Every displayed value carries the provider's credit line.
- **Official documents**: `documents` table + `/api/v1/documents`; Federal Register uses `effective_date` as the request filter and `effective_on` as the response field; the publisher's abstract is never stored and a refresh never overwrites a hand-written summary.
- **Vietnamese feeds**: the `vn` pack ships the VnExpress business feed **disabled**, with the terms to re-read recorded in the file and required attribution stored per source. VnEconomy and Thanh Niên are deliberately absent because their feed URLs and terms are unconfirmed.
- **Nothing has been accepted against a live provider.** No account was created, no key entered and no contact address chosen by this work; the acceptance script requires the owner to do those.

### Safety review of the fetch/collector path (offline, verified by tests)

- Conditional validators (`If-None-Match` / `If-Modified-Since`) are replayed **only to the origin that issued them** and are cleared when a source URL changes (migration 6 stores the issuing origin). Previously they were sent on every redirect hop, including to another host.
- The 5 s per-host gap is now one shared gate used by the scheduler, "Fetch now" and "Test feed"; previously only the scheduler obeyed it.
- A run whose source URL, region or `enabled` flag changes mid-flight is discarded (`status: "discarded"`, `error: "source_changed"`) and writes nothing.
- The parser still tolerates mismatched end tags and a broken tail, but now **requires the feed root element**; an HTML page containing `<item>` markup is no longer accepted as a feed.
- A cancelled run releases its in-flight slot (RAII guard) and stores nothing.

### P3 foundation (official documents and tax)

- **Pack files**: `official.toml` (issuing authorities) and `tax.toml` (tax sources + recurring deadlines) for `global`, `eu`, `eu/fr`, `us`, `vn`. Each file is validated per entry (syntax, duplicate codes, unknown kind, bad cadence, unknown source, out-of-range dates); a bad file is reported and the rest of the pack still loads. Every entry starts **disabled**.
- **Seeding fix**: the built-in pack used to be written only when `region.toml` was absent, so an existing installation would never receive the new P3 files. It now adds missing files and still never overwrites an owner-edited one.
- **Documents**: `topic` and `disclaimer` columns; FTS5 search with Vietnamese diacritics folded (`thue` finds `thuế`), tokens quoted so a query cannot inject FTS syntax, and search hits re-filtered by the same region/topic rules as a listing. A written summary always carries the server's own legal disclaimer; a document without a summary carries none.
- **Tax calendar**: cadence monthly/quarterly/yearly, dates computed in the region's timezone (unknown zone falls back to UTC), the declared weekend rule (`next_business_day`, `previous_business_day`, `none`), day 29–31 clamped to the last day of a short month, `due_soon` from `lead_days`. **Public holidays are not modelled** and both the API and the UI say so. `/api/v1/tax-calendar`; admin `GET /api/admin/tax-rules` and `PUT /api/admin/tax-rules/{code}` (enabling is refused while the rule's region is off).
- **UI**: a Regions page showing news, official documents (with search and topic filter) and the tax calendar for the effective region set, in Paper/Sepia/Night at 375 px.
- **Deadlines in the pack are sample data** (`verified = false`) that only exercises the engine; they are not the real filing dates of any jurisdiction. No reminder is delivered: Telegram/email belong to P7.

## Deployment — Saigon NAS + internal domain (2026-11-11)

- **Repo**: `git init -b main`, first commit `c940cd4` (114 files, no `.local-data`, DB, `target/`, `node_modules`, `.env` or secrets), pushed to Gitea `apps/lnc-newsman` (public) with push-mirror to GitHub `ngojclee/lnc-newsman` (interval 8h, `sync_on_commit`, no mirror errors).
- **Port**: `8462` reserved for `lnc-newsman` on `10.20.2.1` in `lnc-internal-apps` (`registry.json` + `PORTS.md`); `python scripts/check_ports.py` → OK, no duplicate ports.
- **Image**: built **on the NAS from the repo** (`build: .`, repo-build stack 108, GitOps workflow 5 healthy). Chosen over image-pull because the NAS cannot pull from the Gitea registry and there is no verified path to the `docker-build` runner in this change; the stack is a repository stack like `wol-vn1`, so a later push redeploys automatically. Runtime image 90.5 MB, runs as **uid/gid 10001**, `cap_drop: ALL`, `no-new-privileges`, healthcheck on `/health`.
- **Data**: bind mount `/share/Docker/lnc-newsman` (NAS root, not `/home/Docker`); an idempotent `init-data` service creates it and chowns it to 10001 before the app starts, so the app never runs as root.
- **Stack env** (current): `DATA_DIR=/share/Docker`, `PORT=8462`, `TZ=Asia/Ho_Chi_Minh`, `REGIONS_DIR=/data/regions`, generated `MANAGEMENT_KEY`, `LNC_API_KEYS`, `LNC_MASTER_KEY`, `COOKIE_SECURE=true`, `ALLOWED_HOSTS=newsman.lengoc.me,10.20.2.1`, `TRUSTED_PROXIES=10.20.0.3`. Keys live only in `F:\_agentsync\.secrets\newsman\keys.txt` (outside the repo) and are not reproduced here.
- **Login**: `https://newsman.lengoc.me/login.html` (domain applied 2026-11-11 by another session). With `COOKIE_SECURE=true` the session cookie is Secure-only, so **logging in over plain `http://10.20.2.1:8462` no longer keeps a session in the browser** — that address still answers health/API-with-key checks but is not a usable login path. This is deliberate, not a fault.
- **GitOps**: source interval set to **1m** (was default); a push is picked up and redeployed within ~2 minutes (verified with commits `a0a73d2` and `7341a5c`, source/artifact/target all healthy).
- **Verified on the LAN address**: container healthy; `/health` 200; login 200 and session cookie set (401 on a wrong key); `/api/v1/regions` 200 (root `global`; `eu`, `fr`, `us`, `vn` present and **disabled**); 5 sample sources and 7 connectors all **disabled**; `collector_enabled=false`; restart keeps data and stays non-root.
- **Verified through the domain (2026-11-11)**: `https://newsman.lengoc.me/` → 303 to `/login.html`, `ssl_verify_result=0`, `/health` 200, `/login.html` 200; login with the management key 200 and session cookie set; `/api/v1/regions` 200 and `/api/admin/regions` returns all five codes (`global`, `eu`, `fr`, `us`, `vn`) with **only `global` enabled**, effective set `['global']`, pack issues `[]`.
- **Login throttling behind the proxy (fixed 2026-11-11, commit `c81f9df`)**: the limiter used to key on the TCP peer, so behind Caddy every person shared one five-minute bucket. `TRUSTED_PROXIES=10.20.0.3` makes it key on the right-most valid `X-Forwarded-For` entry that the edge appended; caller-supplied entries are ignored and a malformed header falls back to the proxy address. Empty list = trust nobody (old behaviour). Covered by unit tests (`client_key`) and a router-level test that two clients behind one proxy do not share a bucket.
- **Measured 2026-11-12: the production peer is the Docker gateway, so `TRUSTED_PROXIES=10.20.0.3` has never matched and the end-to-end fix is inert in production.** Method: inside the running container (`Portainer exec`, read-only `awk` over `/proc/net/tcp`), sampling only `state 01` (ESTABLISHED) for 22 s while sending traffic two ways in separate windows. Domain only (`https://newsman.lengoc.me/health`, 6 requests): every sample was peer `172.31.0.1` → local `172.31.0.2:8462`. Direct LAN (`http://10.20.2.1:8462/health`, 6 requests): same peer `172.31.0.1` (accepted from different source ports on my machine). So the host-port mapping makes Docker's bridge gateway the TCP peer for **both** the Caddy path and the direct path; the socket-level peer is never the edge. `client_key()` therefore falls back to `172.31.0.1` for everyone and `X-Forwarded-For` is ignored — the shared-bucket problem is back in production even though the code and its tests are correct. This matches what the Library session measured (`172.29.0.1` for stack #109). **No configuration was changed.** Options, each with a cost, are listed for the owner below.
- **Options for the proxy-aware limiter (not applied)**: (a) `TRUSTED_PROXIES=172.31.0.1` — trusts whatever `X-Forwarded-For` any caller sends to `:8462` directly, so anyone on the LAN can dodge the limiter by rotating a header; (b) run the container with `network_mode: host` or publish through a proxy container on the same bridge so the real client IP survives; (c) move the limiter's state to a per-account/per-key basis (the key is a single shared secret, so this only helps if the login form is split per person); (d) accept the shared bucket and make the limit generous (it currently blocks everyone for five minutes after a handful of wrong keys). The honest state today: throttling works as designed at the socket level and is effectively shared at the deployment level.
- **Remaining risks**: `ALLOWED_HOSTS` now lists the domain and `10.20.2.1`, so any other Host header is rejected (400) — a new name needs an env change; the plan-quality gate is still below 8.0; live provider acceptance has not started.
- **Rollback**: `DELETE /api/stacks/108?endpointId=2` removes the stack and containers; data stays in `/share/Docker/lnc-newsman` (delete manually if intended); the Portainer image `lnc-newsman-lnc-newsman:latest` can be removed afterwards. Domain rollback is in `.docs/domain-steps.md` §4.

## UI fix — sidebar and header pinned (2026-11-11, v0.1.1)

- **Symptom**: on a long page the sidebar grew with the content column, so the footer block (version, theme, sign out) sat at the bottom of the document instead of the window; the page header also scrolled away despite `position: sticky`.
- **Cause**: `.sidebar` had `min-height: 100vh` inside the grid, which stretched it to the row height; `.page-shell` had `overflow: auto`, which made that element the scroll container and broke the sticky header.
- **Fix**: desktop sidebar is `position: sticky; top: 0; align-self: start; height: 100vh/100dvh; overflow-y: auto`; `.page-shell` no longer clips or scrolls. Mobile rules (`height: auto`, sticky top bar) are unchanged.
- **Checked in a real browser against the deployed build** (`https://newsman.lengoc.me`, served through a temporary read-only local bridge so the management key never entered the page): desktop 1400x900 at scroll 0/1200/3000 and 375x812 at scroll 2000 — header top stays 0, sidebar top 0, footer bottom stays at the viewport bottom in paper, sepia and night; no horizontal scroll at 375 px.
- **Deployed**: version 0.1.1, commit `b1b5366`, GitOps redeploy; `/health` 200 and `/api/app-info` reports `0.1.1` on the domain; login through `https://newsman.lengoc.me` works and the session cookie is Secure.
- **Not fixed (pre-existing, unrelated)**: at <=900 px the app shows both the horizontal nav and the compact footer row; that is the existing mobile layout and was left as instructed.

## P7 foundation — alert rules and the in-app inbox (2026-11-11, v0.2.0)

- **Chosen because** it is the next phase that could be finished entirely offline: it reads only what is already in the database (articles, P5 labels, regions/topics) and needs no provider, key or network. It also closed the last open item of P5 — labels reaching the reader.
- **Schema** (migration 15): `alert_rules`, `alert_deliveries` with **UNIQUE(rule_id, article_id)**, `alert_skip_log`. The UNIQUE row is what makes "fires exactly once" a property of the database rather than something the sweep has to remember.
- **Behaviour**: cooldown per rule; quiet hours honour a window that wraps midnight and are evaluated in the rule's own timezone; a quiet-hour skip does **not** mark the article delivered, so the event fires after the window instead of being lost; matching uses the same enabled-region and enabled-topic predicates as the reader, so a rule can never notify about an item the reader hides; an unknown `match_kind` matches nothing rather than everything.
- **Channels**: only `in-app`. `telegram`, `email`, `webhook`, `imap`, `slack` are listed in one constant and a rule naming one is refused with `channel_not_implemented` — a stored channel nothing delivers to would look like a working notification while sending nothing.
- **Sweeper**: every 2 minutes, started at the first admin login. Until then all rules are off, and a new install has no rules at all.
- **P5 remainder**: `/api/articles` now returns `labels {topics, entities}` computed in one query per page, and the reader shows a topic chip plus entity chips.
- **Tests**: 182 pass (16 new alert tests + 2 router-level). Covered: second sweep creates nothing; cooldown blocks then expires; 22:00–07:00 quiet window across midnight holds the event and releases it after; an article in a disabled topic never notifies; unknown match kind matches nothing; API refuses an undeliverable channel; mark-read and mark-all-read; labels appear on the article payload.
- **Checked by hand on the live domain** (v0.2.0): `/health` 200; login 200; `/api/admin/alerts/rules` 200 (empty, channels `["in-app"]`); `POST sweep` twice → `created 1` then `created 0` with `already_delivered`; `GET /api/alerts` returns the item; `POST` a rule with `channels: telegram` → 400 `channel_not_implemented`. In a local browser: Alerts page renders, sweep from the UI shows "Đã tạo 1 thông báo", mark-read clears the unread header, reader row shows the topic chip and the `EUR` entity chip, 375 px has no horizontal scroll.
- **Not done**: real Telegram/email/webhook delivery (needs the owner to choose a channel and provide keys), numeric-threshold rules (need real P2 data), story grouping, a nav-bar bell.

## UI pass — buttons, app switcher, mobile shell (2026-11-12, v0.3.0)

Owner reported three things; each was checked in a real browser on a long page before moving to the next.

- **Touching buttons.** Rows were spaced by whitespace between inline elements. Every action row is now a flex/grid container with one 10px gap and wrapping: `.actions-row`, `.section-actions`, `.catalog-form-actions`, `.catalog-entry-actions`, `.regions-actions`, `.alerts-actions`. Under 640px a section heading's action group stacks under its text rather than being squeezed into two buttons that touch. Measured in the browser at 360/375px across all six routable pages: no pair of buttons closer than 6px, wrapping on, no horizontal scroll. Toggles keep their 46×26 look but get a 44px hit area via `::before`; buttons, selects and chips are ≥44px.
- **App switcher.** `ui/src/appLinks.ts` is a byte-identical copy of the Library file (diff clean), so adding an app is a one-file change in both repos. It renders in the sidebar (desktop) and as a row under the brand (mobile), links to `https://ebooks.lengoc.me`, marks the current app active with `aria-current="page"`, and points at the sibling dev port when opened from 127.0.0.1.
- **Mobile shell.** `viewport-fit=cover` plus `env(safe-area-inset-*)` on the top and bottom bars; a fixed 5-item bottom navigation (today, topics, regions, alerts, settings — 52px targets, 61px bar) replaces the horizontal nav strip on phones; `100dvh`; smooth scrolling; inputs at 16px so iOS does not zoom on focus; the bar hides while the on-screen keyboard is open (`visualViewport` + a `keyboard-open` body class). The region picker moved into the page body on phones so a long title can never be squeezed by the header controls — the earlier version pushed the title onto two lines at 360px. Manifest, 192/512/maskable icons, `apple-touch-icon` and the PWA meta tags come from the existing paper-sketch favicon; the 32px raster was checked by eye and stays legible.
- **Verified**: 360×800, 375×812, 390×844; all six pages; paper/sepia/night; no horizontal scroll anywhere; bottom bar present and 61px; input font 16px; `keyboard-open` hides the bar and removing it restores `flex`.
- **Not done**: no APK, no Capacitor or Tauri project. `.docs/mobile-apk-plan.md` is a plan only (Capacitor vs Tauri 2, server-address screen, bearer vs cross-site cookie, Android cleartext, iOS needs macOS) and lists six decisions for the owner.

## UI sync with LNC Library (2026-11-12, v0.4.0)

Two apps, one interface family. The shared conventions were read from Library's `ui-mobile-plan.md` and `ui/src/themes.ts` / `style.css` (U1/U2/W1 blocks on `ui/mobile-redesign`); only layout vocabulary was borrowed, no Library assets.

- **Six reading backgrounds** (Giấy, Sepia, Xám dịu, Xanh dịu, Đêm, Đen) replace the old three. Tokens live on `:root[data-mode=…]` so the page behind the shell and the overscroll area take the same colour; `theme-color` follows the mode. `ui/src/themes.ts` mirrors Library's spec list, and `src/themes.rs` carries the same identifier list for `default_reader_mode` validation.
- **Tab bar** under 900px replaces the desktop sidebar (5 tabs: today, topics, regions, alerts, settings), using Library's names (`--tabbar-h`, `--safe-t`, `--safe-b`, `--touch`, `.tabbar`, `.tab`).
- **One `.btn-row`** for every action row (`.actions-row` kept as an alias), so spacing comes from `gap` and rows wrap.
- **Desktop browse panel** (`W1`): quick filter plus topics / regions / sources with reader-visible counts, and a filter chip on the Topics page that clears back to everything. The article list honours a source filter, which needed one new query parameter.
- **Home is rails now** (`W2`): Latest, Recent alerts and Following, each a horizontal strip built from endpoints that already exist (`/api/articles`, `/api/alerts`). No new server state.
- **Backend additions**: `GET /api/articles?source=<slug>` (applied on top of the same visibility rules) and per-source counts in `GET /api/dashboard` (`collector::source_facets`).
- **Version**: `VERSION` is the single source (`0.4.0.0`); the UI bakes it in at build time. The Docker UI stage copies only `ui/`, so `vite.config.ts` also accepts `VERSION` beside `package.json` and falls back to the package version — the first redeploy of this change failed on exactly that and was fixed in `cb09bc0`.
- **Checked in a real browser** (local debug build on `127.0.0.1:8000`, synthetic local rows only): 1280×800 and 375×812; all six modes measured by background/text colour (paper `#f6f1e8`, sepia `#eadcc6`, gray `#e6e6e3`, green `#dfe9dc`, night `#211f1c`, black `#000000`), no horizontal scroll in any; tab bar 64px with 5 tabs, every tab ≥44px; sidebar browse narrows 12 items to 3 for "Tài chính" and the chip clears it; Settings theme grid paints all six swatches and `theme-color` becomes `#000000` in Black.
- **Deployed**: commits `9a80968` + `cb09bc0`, GitOps target healthy; `/api/app-info` reports `0.4.0`; `/health` 200; login through the domain 200 and `/api/v1/regions`, `/api/dashboard` both 200 with the session.
- **Not done**: no APK. `.docs/mobile-apk-plan.md` is now aligned with Library's plan (Capacitor, server-address screen, bearer read-only, no cross-site cookie, iOS as PWA) and states one shared set of decisions.

## P8 offline part — read-only MCP endpoint (2026-11-12, v0.4.0)

Chosen as the next phase that is **fully doable offline**: `/mcp` wraps data the app already stores and needs no provider, Hub key or network. Its tools call the same functions `/api/v1/*` calls, so it cannot widen visibility.

- **`POST /mcp`** speaks JSON-RPC 2.0 (`initialize`, `ping`, `tools/list`, `tools/call`, batches answered in order) and takes the same credentials as REST v1 — session cookie or a read-only API key. **`GET /mcp` answers 405 `stream_not_supported`** rather than pretending to open a server-sent event stream.
- **Seven read-only tools**, namespaced `newsman__`: `regions`, `topics`, `latest_articles`, `documents`, `tax_calendar`, `series`, `observations`. Each one names the REST endpoint it mirrors. **No write tool and no tool for data that does not exist yet** — `market_snapshot` (needs live provider data), `region_brief` (P6) and an alert tool are deliberately absent, because a tool that answers "no data" for months is worse than a missing tool.
- **Error shape**: an unknown method → JSON-RPC `-32601`; an unknown tool → `-32602`; a bad argument or a disabled region → `result.isError = true` with a message, so a client shows it without treating the call as a transport failure.
- **Refactor**: `documents::list`, `series::list_in` and `regions::payload` are now the single implementations both REST and MCP call. `series::list_series` and `documents::list_documents` only resolve the region and format the response.
- **Docs are now a test**: `src/docs_tests.rs` parses every `.route("…")` in `src/main.rs` and every `/api/…` token in `API_DOCS.md`; a route without a doc line, a doc line with no route, a `mcp_available` flag that disagrees with the `/mcp` route, or an `/mcp` section missing from the docs all fail `cargo test`. This is the P8 item "API_DOCS matches 100% of routes", enforced instead of asserted. The alert routes (P7) were missing from the docs and are now written down.
- **Tests**: 192 pass (7 MCP + 2 docs). Covered: 401 without and with a wrong key; `initialize`/`tools/list` stable across calls and containing no write tool and no duplicate name; a tool's payload equals the REST endpoint's payload for the same query; missing argument, unknown tool, disabled region and unknown method each take their intended error path; a batch answers every call in order; `GET /mcp` is 405.
- **Checked by hand on the local build** (`127.0.0.1:8000`, demo key, synthetic rows): `initialize` returns protocol `2025-06-18`, `serverInfo.name` `lnc-newsman`, version `0.4.0`; no key → 401; `tools/list` returns the seven names; `newsman__latest_articles{limit:2}` returns `isError:false` with two items.
- **Still open in P8**: registering the endpoint with the Agent Hub (needs the owner's Hub key and a connector record) and any write surface — administration stays on the session-authenticated web UI.

## P8 connector half — Business MCP Hub boundary (2026-11-12, v0.4.4)

The other half of P8: the endpoint can now be mounted on the Hub. The Hub is MCP-native and proxies `tools/call` to our own `POST /mcp`, so the work was a **credential and scope boundary**, not a second API.

- **Two credentials, two provenances.** The Hub sends an app-specific `Authorization: Bearer <connector token>` **plus** five signed delegation headers (`X-MCP-Hub-Client-ID/Scopes/Timestamp/Nonce/Signature`, optional `-App-ID` for the v4 envelope). The bearer alone would let the Hub do anything with the connector's full rights, so **every tool decision is made from the signed scopes**, never from the bearer.
- **`env_or_cfg`, both directions.** `CONNECTOR_BEARER` / `DELEGATION_SECRET` (legacy `LNC_CONNECTOR_TOKEN` / `LNC_DELEGATION_SECRET` still read) / `CONNECTOR_AUDIENCE` / `CONNECTOR_MAX_SCOPES`: env wins when set; otherwise the value is read from the existing sealed store (`connector-bearer`, `delegation-secret`) or from `settings`. That means the owner can pair from `/admin` **without a redeploy**, and an operator can pin both at deploy time. Verified live: with env blank, pasting both values into `/admin` flipped `enabled` to `true` and a signed `tools/list` then answered 200 on the same running process.
- **Reject paths, all measured over real HTTP, not just unit-tested**: wrong token → 401 `connector_auth_required` (an unpaired connector answers identically, so a caller cannot tell misconfiguration from a wrong token); bad signature, wrong audience, timestamp >90 s, replayed nonce, or a scope outside the ceiling → 403 `delegation_denied`. A nonce is only claimed **after** the signature verifies, so an unauthenticated caller cannot burn one.
- **Tool names are now bare** (`regions`, not `newsman__regions`). This was a real bug waiting to happen: the Hub prepends its namespace, so the old names would have reached an agent as `newsman__newsman__regions`. `newsman__<tool>` is still **accepted** (prefix stripped before lookup) so anything written against 0.4.3 keeps working — proved by test.
- **Unknown tool and out-of-scope tool answer identically** (`-32601`, "Unknown or unauthorized MCP tool"), and `tools/list` is filtered by the caller's scopes, so probing the namespace reveals nothing.
- **No write tool exists to grant.** `CONNECTOR_MAX_SCOPES` accepts only `newsman:read`; anything else is refused at write time with `unsupported_connector_scope` rather than silently narrowed, so the ceiling cannot look wider than it is.
- **CSRF, deliberately and narrowly.** A Hub call cannot send `X-Requested-With`, and CSRF is an attack that rides a *browser's* credential — which a bearer-carrying request does not have. The exemption is `POST /mcp` plus the connector bearer, nothing else. An unauthenticated `POST /mcp` also skips it so the honest answer is the credential gate's 401 instead of a CSRF complaint; every other route keeps the guard unchanged.
- **Tests: 237** (up from 192), clippy `-D warnings` clean, `npm run build` clean. `src/connector_tests.rs` writes the envelope out by hand rather than reusing `envelope()`, so a change to the signed bytes fails a test instead of passing silently.
- **Admin card** (`/admin`, "Business MCP Hub connector"): shows whether each value is set and **where it came from** (`env` / `admin` / `unset`) — never a value. The two secrets are not entered there; they go through the existing provider-keys panel, which is already write-only.
- **Deployed**: GitOps was wedged for ~10 min (`lastSyncDate: 0`, three workflows `syncing` at once, zero Docker image/build events in an hour) and cleared on its own — **no Portainer restart, no stack stop, no `stackFileContent`**. Live: `/api/app-info` `0.4.4`, container `4035d7430f09` healthy, stack status 1. Verified on production: no credential → 401 `unauthorized`; a Hub-shaped call before pairing → 401 `connector_auth_required`; the reader API key still lists all seven bare-named tools and `regions` returns data.
- **Not done**: the connector record on the Hub itself (the owner creates it from the pairing values in the hand-off) and enabling it there. No record was created, no key printed, no grant changed.

## P8 write surface — notes, briefs and flags for Hub agents (2026-11-12, v0.5.0)

Owner's decision: let an outside agent write analyses and notes through the Business MCP Hub, and nothing more.

- **`newsman:write`** (new scope) unlocks six tools: `note_create`/`note_update`/`note_delete`, `analysis_create`/`analysis_update`, `article_flag`. Three read tools were added too (`notes`, `analyses`, `write_log`). A write tool needs **both** scopes: writing a note on an article means seeing that article first.
- **The boundary is a shape, not a blocklist.** Every write tool touches one of three new tables (`newsman_notes`, `newsman_analyses`, `newsman_article_flags`); sources, regions, connectors, secrets, the collector switch and alert delivery are not reachable at all, so they stay behind `/admin`. A test asserts no write tool is named after a configuration surface.
- **The author is the verified delegation**, never an argument. `author_client` comes from the `Caller` the boundary built, and ownership is enforced on update/delete: a note the owner wrote in the web UI (`author_client` NULL) and one another client wrote are both refused.
- **A real bug found by measuring instead of trusting the schema**: `additionalProperties: false` is a note to the *client*, not something an MCP server enforces. `{"author":"louis"}` was silently dropped and the call succeeded, so an agent would believe its forgery worked. The server now refuses an undeclared argument (`unknown argument: author`), with a test and an HTTP smoke case.
- **Every write is audited** in `newsman_write_log` (time, client id, app id, tool, record kind, record id), readable through the `write_log` tool and `/api/admin/connector/writes`. Metadata only — never the note text.
- **Ids resolve against visible rows**: a note needs an existing article/document, a cited article id must be stored, a brief cannot name a disabled region or topic, lengths are bounded (`too_long`, `too_many_sources`, `invalid_sources`).
- **`GET /api/admin/connector/analyses`** and the admin card show the **author label** so the owner can tell agent output from their own.
- **Tests: 254** (11 write tests + 1 argument test), clippy `-D warnings` clean, UI build clean. Measured over real HTTP: no scope → `-32601`, with scope → note stored and attributed to the client id, forged author → refused, bad citation → refused, audit row present, reader API key path unchanged.
- Migration **18** is additive only (four `CREATE TABLE`, no `DROP`, no `DELETE`); verified applying cleanly on a fresh database before deploying.

## 0.5.0 deployed and verified (2026-11-12)

Live at `newsman.lengoc.me`: `/api/app-info` 0.5.0, container `2d0802e6f125` healthy, stack 108 `up`, GitOps workflow `healthy`. Verified against production, not the local build:

- `tools/list` with a reader key returns **16 tools** with bare names (7 read, 3 new read, 6 write);
- no credential → 401 `unauthorized`; a Hub-shaped call before pairing → 401 `connector_auth_required`; an undeclared argument → `isError` with `unknown argument: limit`;
- `/api/admin/connector`, `/api/admin/connector/writes`, `/api/admin/connector/analyses` all 200 — **migration 18 applied** to the production database.

**Two things this deploy caught, both worth remembering:**

1. `stack.yml` still defaulted `CONNECTOR_MAX_SCOPES` to `newsman:read`. The code granted writes while the deployed connector advertised a read-only ceiling — a ceiling that disagreed with the owner's decision, and quietly. Fixed in `c6995e2`; the compose default is now both scopes, and a deployment that wants read-only sets the env.
2. A build of the whole Rust release takes **~15 minutes** on this NAS, and `AutoUpdate.Interval: 1m` means **every push to `main` starts one**. Commit in batches; a handful of small docs commits will occupy the queue for the better part of an hour and delay anything else building on the same daemon.

## 0.5.1 — Obsidian feedback loop, deployed and verified (2026-11-12)

Closes the second half of the Obsidian loop: an opinion written under a forecast in the vault comes back as a note attached to that analysis.

- **Migration 19 rebuilds `newsman_notes`** so a note can target an analysis. SQLite cannot alter a `CHECK`, so the table is copied, dropped and renamed inside the transaction the migration runner already opens; migration 18 is left untouched. The `CHECK` is now "exactly one of three targets".
- **`note_create` / `notes` take `analysis_id`**; `analyses` gained `stable_key` (`analysis-<id>`, from the immutable row id, so it survives edits) and `updated_since` (ISO date at UTC midnight, or Unix seconds; strictly after; an unusable value is an error rather than a filter that quietly returns everything).
- **Production verification, all measured after the swap to container `04a2c8ce298f`:**
  - `/api/app-info` = **0.5.1**; stack hash `7858a9d`.
  - Row counts **before → after** the migration: notes 0 → 0, analyses 0 → 0, flags 0 → 0, write_log 0 → 0, articles 100 → 100.
  - Schema is 19, `newsman_notes` has `analysis_id`, and the new `CHECK` is present.
  - `analyses` returns (empty in production — no analyses exist yet); `updated_since` with `1970-01-01` and `2099-01-01` both filter correctly; `"not a date"` is an `isError`.
  - `notes {analysis_id: 1}` answers `{data: [], flags: []}` — the third target is readable.
- **Settled by the owner's lane, so nobody "fixes" it later**: a write through `/mcp` requires a delegated Hub caller **by design**. An owner web session writing there would make `author` ambiguous — a note an agent created and one the owner typed would become indistinguishable, and the `client_id` audit trail would have a hole. Owner writes go through the web UI's own endpoints, not MCP. Do not relax this.
- **Backup taken before the deploy**: `/data/newsman.db.pre-0.5.1` and `newsman.db-wal.pre-0.5.1` in the previous container. The production tables were empty, so the rebuild had nothing to lose — but the backup stays until the owner says otherwise.
- **Not verifiable yet, and worth knowing**: the two write-guard cases (`note_create` with a missing `analysis_id`, and with two targets) could not be exercised on production because **the connector is not paired** — every write tool requires a delegated caller, so an owner web session is refused with `this tool requires a delegated caller`. That is a real behaviour, not a bug introduced here: it has been true since 0.5.0 and matches "the author is the verified delegation". Both guards are covered by unit tests and were measured live on a local build.

## 0.6.0 — Live Business MCP Hub pairing verified + Reader briefs, grounded analyst, starter presets & connector scheduler (2026-11-12)

1. **Business MCP Hub live pairing verified end-to-end (production `0.5.1` + Hub `10.21.4.101:8790`):**
   - Verified `newsman`, `obsidian`, and `lnc-library` connectors registered and enabled on Business MCP Hub with live signed delegation (`newsman:read`, `newsman:write`).
   - Exercised live MCP round-trip through the Hub (`hub_cache_refresh` → `newsman__regions`, `newsman__topics`, `newsman__latest_articles`, `newsman__analyses{limit:3, updated_since}`, `newsman__note_create` → `newsman__notes` → `newsman__note_delete` → `newsman__write_log`, plus `obsidian__vault_status` and `obsidian__vault_list` confirming `_agents/` boundary). All test rows cleaned up afterward.
2. **Provider connector controls & background scheduler (`src/connectors.rs`):**
   - Added `PUT /api/admin/connectors/{id}` (`{"enabled": bool}`) validating offline gates (`region_disabled`, `missing_secret`, `secret_store_locked`, `secret_corrupt`) before flipping `enabled`.
   - Added `connectors_auto_enabled` setting (default `false`, separate from RSS `collector_enabled`), `ConnectorInFlight` concurrency guard (`409 connector_busy`), and `connectors::spawn_scheduler` polling due connectors every 60 s.
   - Upgraded `/admin` Connectors card with Enable/Disable, "Lấy ngay" (`Run now`), and automatic connector scheduler toggle.
3. **1-click curated starter RSS/Atom presets (`src/catalog.rs` + `CatalogManager.tsx`):**
   - Added 10 vetted official/public starter feeds (`GET /api/admin/sources/presets` and `POST /api/admin/sources/presets/{slug}/add`) across `global`, `eu`, `fr`, `us`, `vn` (SEC Press Releases, Fed Monetary Policy, Treasury Press Releases, ECB Press, CNIL France, ANSSI CERT-FR, Hacker News Frontpage, GitHub Blog, Rust Blog, VnExpress Kinh doanh).
   - Presets register **disabled** (`enabled = false`) with required attribution and perform zero network calls until the owner enables the region and source.
4. **Reader Reports (`#reports`), Today briefs rail, two-way feedback notes, Focus/Schedule preferences & grounded Hub research (`src/reader.rs`, `src/mcp.rs`, `ui/src/main.tsx`):**
   - Added REST v1 read endpoints (`GET /api/v1/analyses`, `GET /api/v1/analyses/{id}` with resolved `cited_articles` and `notes`, `GET /api/v1/notes`) and owner session write endpoints (`POST /api/notes` with `author_client = NULL` protected from agent edits, `DELETE /api/notes/{id}`, and `POST /api/admin/research` grounded in stored headlines, macro/market series, declared tax deadlines, and `focus_profile`, with optional `save_as_brief: true`).
   - Added `focus_profile`, `brief_morning_time`, `brief_evening_time`, `brief_timezone` in `/api/settings` and exposed read-only MCP tool `reader_preferences` (`newsman:read`, bringing `/mcp` to **17 tools**: 11 read + 6 write).
   - Unlocked `#reports` (`Báo cáo`) in the reader shell, added Latest Briefs rail on `#today`, Focus & Schedule panel in `#settings`, and updated stale MCP status copy.
   - `src/connector.rs` untouched; 270 Rust tests pass, `API_DOCS.md` 100% in sync with routes and MCP tools (`docs_tests`), TypeScript + Vite build clean. Local `0.6.0` ready for owner review before pushing to `main`.

## Infinite scroll (2026-11-12, v0.5.0)

Owner's request: drop the "Xem thêm" button and load the next page when the list reaches the bottom.

- One hook, `useInfiniteScroll`, used by every paginated list. The REST API is offset-based, so **only the trigger changed** — no cursor, no new endpoint.
- **One attempt per rendered page**: the observer disconnects on firing and the effect rebuilds it only after the list grew or a load finished, so a sentinel left in view cannot fire twice.
- **A load that adds nothing stops the loop** and brings the button back with "Không tải thêm được — thử lại". A failed request must not become an invisible spin.
- The sentinel is rendered only when a page can still arrive (a capped home rail never gets one), and a browser without `IntersectionObserver` keeps the button.
- **Measured in a real browser** against 120 stored articles: 30 → 60 → 120 while scrolling, switching section resets instead of appending, and a failing page request restores the button and stops.
- **Tooling note worth keeping**: the in-app browser pane **defers `IntersectionObserver` callbacks** for a hidden page and while the page is scrolled by script — a control observer on a fully visible element also reported zero. The count only moves with a real scroll (`computer{action:"scroll"}`) and the pane displayed. Do not conclude the code is broken from a scripted measurement.

## Vận hành: đọc trạng thái deploy của stack 108 (bài học 12/11/2026)

Mỗi push vào `main` của `lnc-newsman` **kích một build Docker đầy đủ**: stack là repo-stack
(`Type: 2`) với GitOps source poll `1m` — không cần webhook (repo có **0** webhook). Một
build Rust release trên NAS mất **>14 phút**, nên push dồn dập sẽ tự xếp hàng sau chính nó.

Ba nguồn **không** dùng để kết luận "không có build":

| Nguồn | Vì sao sai |
|---|---|
| `GET /api/endpoints/2/docker/images/json` | Image chỉ xuất hiện khi build **xong**; build đang chạy thì không thấy gì |
| `GET /api/endpoints/2/docker/events?type=image` | Trả **rỗng** cả khi build đang chạy — đã kiểm, vô dụng làm bằng chứng phủ định |
| `AutoUpdate.Interval` | Là chu kỳ poll, không phải "mỗi push một webhook" |

**Cả "log không nhích trong 20 giây" cũng không phải bằng chứng**: `cargo build --release`
ở bước cuối im lặng nhiều phút. Tôi đã dùng đúng phép đo đó để kết luận "build đã chết" và
sai — build vẫn chạy bình thường.

Nguồn **đúng** — log của container Portainer, đọc qua Docker proxy (read-only):

```
GET /api/endpoints/2/docker/containers/<portainer-id>/logs?stdout=1&stderr=1&tail=120
```

Trả về frame 8-byte (4 byte stream + 4 byte big-endian length) rồi tới payload. Build đang
chạy thì thấy buildkit ở đó, ví dụ `#25 186.9 Compiling lnc-newsman v0.5.0 (/src)`.

Cũng đừng kết luận từ `ConfigHash` một mình: trong lúc build, nó vẫn ghi **commit cũ** —
Portainer cập nhật hash khi build xong, không phải khi bắt đầu. `Status: 3` còn tệ hơn: một
stack đã build xong và swap container vẫn có thể còn `Status=3` một lúc.

**Và một biến thể nữa, do session Library chỉ ra:** "tôi không lấy được lock" không phải
"lock bị kẹt". Một lệnh thất bại lặp lại trông giống hệt một hệ thống hỏng. Cách phân biệt
là tìm một thao tác **khác** chứng minh tài nguyên còn dùng được — ví dụ `PUT
/api/stacks/108/git/redeploy` trả 200 — chứ không lặp lại thao tác đã thất bại.

**Phép đo đáng dùng là loại mà kết quả âm là bằng chứng thật.** Có image mới xuất hiện,
hoặc container bị thay (id khác đi) — đó là bằng chứng không thể nhiễu. "Không có gì mới"
thì có hàng chục cách giải thích.

## P7 Telegram channel (2026-11-12, v0.4.1)

Requested by the owner through the UI lane: alerts into Telegram, two configuration paths, one decision shared with LNC Library (`.docs/l8-agent-chat-telegram.md`, part B).

- **The relay was read before a line of client code was written.** `telegram.lengoc.me` is `cygnus-gateway` (read at **0.2.5.1**, live at **0.3.0.0** — the send path is unchanged between them): `GET /health` answers `{ok, service, version, bots_count}`; **`POST /send`** takes either a flat Bot-API body `{chat_id, text, bot?, …}` or `{method, payload}` where `method` defaults to `sendMessage` and must be in the gateway's `allowed_methods`. Auth is an API key (`cyg_*`) via `Authorization: Bearer`, `x-token` or `?token=`. `payload`/the remaining fields are forwarded to the Bot API verbatim, so `chat_id` **and `message_thread_id`** (forum topic) pass through unchanged — no relay change is needed. **The key can be pinned to one bot**: naming another bot is a 403, so we never send a `bot` field and let the pinned key decide. There is no `/api/send`; every `/api/*` route needs a management session and is not used. Inbound is a bot poller with text-prefix routes POSTing the full update to a route URL (`x-gateway-key`) — that direction belongs to the Library lane and to a future Newsman inbox, not to alert delivery.
- **`telegram_mode`** = `relay` | `direct` | `off`, `env_or_cfg`: `TELEGRAM_MODE` wins when set, otherwise the stored setting applies. `TELEGRAM_RELAY_BASE_URL` (https only, default the fleet relay) and `TELEGRAM_ALLOWED_HOSTS` complete the env half; the fleet relay host is always allowed, so it works without extra configuration.
- **Secrets stay write-only** in the existing vault: `telegram-relay-token` and `telegram-bot-token`. Direct mode builds `https://api.telegram.org/bot<token>/sendMessage`; the `Endpoint` type has a hand-written `Debug` so a log line cannot print that URL. Nothing is ever echoed back — `/api/admin/telegram` and `/api/settings` report only whether each secret is stored.
- **`alert_deliveries` is unchanged.** The sweep writes the in-app row as before and returns which deliveries ask for Telegram; the caller queues an outbox row (`alert_telegram_outbox`, migration 16) **after** the database lock is dropped, and a 30-second flusher sends it. A row is claimed `pending → sending` in one transaction before any request, so two flushers cannot both send it, and a row stranded by a crash is reclaimed after 300 s. Five attempts, then `failed` — visible in the admin queue counts instead of retrying forever.
- **The message is the article title and its link**, exactly what the inbox shows; `alerts_topic_id` (optional) sends into a forum topic, otherwise General.
- **Naming the channel does not require a secret yet.** Once `telegram_mode` is not `off`, a rule may declare `telegram`; a missing chat id or secret shows up as `reason` (`telegram_chat_id_missing`, `telegram_secret_missing`) on `/api/admin/telegram` and as queued rows, rather than as a rule the admin cannot save. The rule list offers exactly the channels a rule may name right now.
- **UI**: a "Kênh Telegram" card in Settings — mode select, chat id, alert topic, whether each secret is stored, ready/not-ready with the reason, queue counts and a "Gửi ngay" button.
- **Sidebar fix (owner-reported)**: `.side-shelves` is a stretched grid inside the sidebar's flex column, so the default `align-content: stretch` divided its spare height across every row and inflated the filter box and shelves (85 px where the spec is ~37 px). One line — `align-content: start` — was applied; measured at 1440×900 afterwards: filter input 37 px, "Mọi tin đang đọc" 37 px, group head 33 px, item 33 px.
- **Verified**: 203 tests pass (11 new Telegram ones plus the rule-validation cases); clippy `-D warnings` clean; UI build clean. In a local browser: settings round-trip, a bad chat id refused with `invalid_telegram_chat_id` before anything is written, unknown settings fields still 422, `telegram` accepted by the rule API only while the mode is on, a real sweep queued one row and the flush attempted it against the live relay (rejected with the deliberately fake token — the request path, not the relay, was what was under test). No real send to a chat: no bot or chat id exists for Newsman yet.
- **Not done**: no bot has been chosen on the relay, so `telegram_mode` stays `off` in production until the owner names one; inbound messages/topic routing belong to the Library lane (this app only sends).
- **Deployed**: commit `df5f64d`, GitOps target synced. A redeploy does not rebuild the image on this repo-build stack (the container was recreated from the existing image, still 0.4.0); a **stack stop/start** rebuilt it — `PUT /api/stacks/108/stop` answered 409 while GitOps was mid-sync and succeeded once the target was idle. Live afterwards: `/api/app-info` `0.4.1`, `/health` 200, `/api/admin/telegram` 401 without a session, `/api/admin/telegram/flush` and `/api/admin/alerts/sweep` 403 without the CSRF header (same as the existing admin routes), and the served CSS contains `align-content:start` on `.side-shelves` while the served JS contains the "Kênh Telegram" card.

## P8 legal-text sources — terms gate before any connector (2026-11-12, v0.4.2)

Requested by the owner through the Finance Overview lane: Newsman detects new legal texts and alerts on them; Library keeps the full text (`.docs/legal-sources.md` in `lnc-library-ui`, §8). The instruction carried a hard rule — **verify each source's terms before retrieving anything, no bulk download, no commercial mirrors** — so this step implemented the gate first and a connector second.

- **Terms research, 12/11/2026.** Four sources, four different answers, all recorded with the URL, the date and a verbatim quote:
  - **govinfo (US)** — *not required*: `govinfo.gov/about/policies` says public documents "can generally be reprinted without legal restriction" and "customary credit should be given". The GPO also documents per-collection RSS feeds (`govinfo.gov/feeds` → `/rss/<collection>.xml`), keyless.
  - **Légifrance (FR)** — *verified*: `legifrance.gouv.fr/contenu/pied-de-page/open-data-et-api` states reuse is free under **licence ouverte 2.0** plus the PISTE CGU plus the API CGU plus quotas on the PISTE portal. Registration and an OAuth2 key are mandatory, and **no key exists**, so its `endpoint` stays empty.
  - **EUR-Lex (EU)** — *unverified*: its legal-notice pages answered **HTTP 202 with an empty body**, and a documented feed URL answered "Service temporarily not available". The Commission's own notice (CC BY 4.0) is not the EUR-Lex notice, so it was not substituted.
  - **vbpl.vn (VN)** — *unverified*: no terms-of-use page exists that could be read (the portal renders client-side and the footer carries none). `robots.txt` allows the rest but **disallows `/api/` and `/Pages/`**. The copyright analysis in `legal-sources.md` §3 stands — Vietnamese legal texts are outside copyright protection (Điều 15 khoản 2) — but *that is not the site's terms*, and the two are different questions.
- **The gate is data, not a comment.** Migration 17 `legal_sources` stores `terms_verdict` (`verified` / `not_required` / `unverified` / `prohibited`) with `terms_checked_on`, `terms_url`, `terms_quote`, `terms_note`, and a `CHECK` constraint. `legal::verdict_allows_polling` accepts only the first two. `set_enabled` refuses `terms_unverified`, `terms_prohibit_polling`, `no_documented_channel` and `region_disabled`. Every source ships **disabled**, and a re-seed never re-enables or overwrites a row the owner changed.
- **One real channel.** govinfo is the only source with a channel that could be verified, so it is the only one with an endpoint. Enabling it creates the collector row `legal-govinfo` (T1, every 12 h) in the same call, which means alerts, topic classification and the reader all work without any new fetching code — `feed::parse` already keeps metadata only. `/api/admin/legal-sources` and `PUT /api/admin/legal-sources/{code}` expose the state; the Settings card shows the verdict, the checked date, the reason a source is not followed, and a folded "Điều khoản" panel with the quote and its URL (`evidence_age_days` / `evidence_stale` flag a verdict older than 180 days).
- **Proven end to end, then turned back off.** With `us` enabled and govinfo followed, `POST /api/admin/sources/7/fetch` stored **100 public laws** (`items_new: 100`), and the reader returned them with the GovInfo source name, tier T1 and the original `govinfo.gov/app/details/...` link. The article text was never fetched — only title, link, guid and date. Afterwards the probe was reverted: govinfo switched off, `us`/`eu` regions off again, the 100 articles and the `legal-govinfo` source row deleted, leaving the DB as it was.
- **Two real UI bugs found while reviewing this screen** (the shared checklist item U): (1) Lightning CSS **drops `backdrop-filter` from the build**, so the mobile tab bar kept a 6 %-transparent background with no blur and page text showed through it — it is now opaque, measured at 375×812 with `elementFromPoint` and a screenshot; (2) a source could offer "Bật theo dõi" even though the server must refuse it, so the button now appears only when it will succeed, with a short status word instead ("Chưa kiểm điều khoản", "Chưa có kênh truy cập chính thức").
- **Verified**: **217 tests** pass (13 new legal-source ones, plus a test that pins the *real* govinfo feed shape); clippy `-D warnings` clean; UI build clean; no horizontal overflow at 375 px in the new card.
- **Not done, on purpose**: no connector for Légifrance (no PISTE key), EUR-Lex or vbpl.vn (terms unread). Production ships with all four sources disabled, so nothing is polled until the owner reads and accepts the evidence in the card.

### Enabling govinfo on production — blocked by NAS egress (12/11/2026)

The owner approved switching govinfo on. The two steps were done in the documented order and then **reverted to off** when the first fetch failed for a reason outside the application:

- **Sequence worked as designed.** `PUT /api/admin/regions/us` → 200, then `PUT /api/admin/legal-sources/govinfo` → 200, and the collector row **`legal-govinfo` (id 6, tier T1, every 12 h, topic politics)** appeared in the same call. Both switches were then set back to `false` (200 each); all four legal sources are disabled again, exactly as v0.4.2 shipped.
- **The first fetch failed at the network layer, not in the parser.** `POST /api/admin/sources/6/fetch` returned `{"status":"error","error":"timeout","items_new":0}` after **10 575 ms** — matching the fetcher's 10 s `connect_timeout`. `consecutive_failures` went to 1 (auto-pause is at 5).
- **Measured inside the container** (`lnc-newsman`, 172.31.0.2): DNS resolves fine (both A and AAAA), but every outbound TCP connect fails — `1.1.1.1:443`, `8.8.8.8:53`, Cloudflare's address for `www.govinfo.gov`, the LAN firewall `10.20.0.1`, another VM `10.20.1.105:9443`, and even its own gateway `172.31.0.1`. A container-to-container call on the **same host** (`172.18.0.3:8081`) succeeds.
- **Control group, same host, same commands, same moment**: `immich_server` 172.19.0.5, `kavita` 172.21.0.2, `vaultwarden` 172.22.0.2, `lazylibrarian` 172.18.0.4, `fail2ban` 172.28.0.2 and `couchdb` 172.17.0.2 all reached govinfo with **HTTP 200**; `wolweb` (network `host`) did too. Failing: `lnc-library` 172.29.0.2, `lnc-cwa-trial` 172.30.0.2 and `lnc-newsman` 172.31.0.2. **Every failing subnet was created in the last ~40 h; the working ones are older.**
- **Ruled out**: MTU (all bridges 1500), global forwarding (`ip_forward=1`, per-bridge `forwarding=1`), link state (all `up`, `carrier=1`), DNS (resolves correctly), and the application itself (identical binary and config as the local end-to-end proof, which fetched 100 public laws).
- **Not established directly**: `iptables -t nat -S` read through the `netbird-nas` netns shows only empty default chains, but that command warns that **iptables-legacy tables are present** and the ASUSTOR ADM host uses a non-standard table setup, so the NAT rules were never read first-hand. A read-only diagnostic session on the NAS should confirm it.
- **A clean exit-code matrix narrows the mechanism** (`0` open, `1` refused, `124` timeout), run against three failing (`172.29`, `172.30`, `172.31`) and three working (`172.17`, `172.18`, `172.19`) containers, same six targets:

  | Target | Failing subnets | Working subnets |
  |---|---|---|
  | Portainer on the host itself — `10.20.2.1:19943` | **0** | 0 |
  | A closed port on the host — `10.20.2.1:22` | 1 (refused) | 1 |
  | A container on another bridge, same host — `172.18.0.3:8081` | **0** | 0 |
  | OPNSense — `10.20.0.1:53` | **124** | 0 |
  | Another VM — `10.20.1.105:9443` | **124** | 0 |
  | `1.1.1.1:443` | **124** | 0 |

  Reaching the host itself and reaching another bridge both work, so **bridge-to-bridge forwarding is fine**; only destinations **off the host** time out. That is the signature of a source network without masquerade — the reply has no route back. It makes the missing-NAT explanation fit the evidence strongly, while still being inferred from behaviour rather than read from the rule set.
- **Not changed**: no DNS, Caddy, Cloudflare, firewall or Portainer stack configuration was modified. The only changes were the two application switches, both reverted. No articles were stored.
- **Next step belongs to the owner/infra**: restore outbound NAT for the new bridge networks on the Saigon NAS, or run Newsman with `network_mode: host` (as `lnc-social-dl` and `wolweb` do). Once egress works, re-enabling is the same two buttons and the fetch should store ~100 public laws as the local proof did.

### Govinfo live, and two real labelling bugs it exposed (2026-11-12, v0.4.3)

NAS egress was restored (17 masquerade rules, one per bridge) and the two switches were flipped again: `PUT /api/admin/regions/us` → 200, `PUT /api/admin/legal-sources/govinfo` → 200, `POST /api/admin/sources/6/fetch` → **`{"status":"ok","items_new":100,"items_seen":100}` in 578 ms**, against 10 575 ms of connection timeout before. The reader returns them with the GovInfo source name, tier T1, region `us`, topic `politics` and the original `govinfo.gov/app/details/...` link; `published_at` is the feed's own date. Only title, link, guid and date are stored — never the PDF, HTML or USLM files the feed's `<description>` links to. The source runs every 12 h with `validators_origin` set, so the next pass revalidates instead of refetching.

Two genuine bugs surfaced only because 100 real items arrived, and both are fixed:

- **Every ordinary capitalised word was labelled a stock ticker.** Entity extraction uppercased the title before matching, so a Title-Case headline turned every word into an all-caps token: all 100 articles carried `TICKER:LAW`, 97 carried `TICKER:ACT`, and 147 distinct "entities" were mostly English function words. Two rules replace the guesswork: a symbol must be **genuinely capitalised in the source** (`TICKER:AAPL` in "lifts AAPL" survives; "Public" and "Rural" do not), and a title written **entirely** in capitals says nothing about which words are symbols, so it yields no tickers at all. Against the same 100 titles the noise fell from ~100 rows to 12, and those 12 are real acronyms inside statute names (`FISA`, `LACA`, `GI`, `VA`, `ROAD`) plus `ORG:` matches — only Roman numerals (`Title VII`) were still wrong, and those are now excluded. The long blocklist that first looked necessary was cut back: it is no longer a list of English words, only of what stays ambiguous when genuinely capitalised.
- **`reclassify_all` could not undo its own work.** Its doc comment claimed it was safe to repeat, but `store` only upserts, so a rule change left the old labels in place — the 100 articles would have kept `TICKER:LAW` forever. It now clears that article's rule labels before re-applying them. A `human` label is still never touched, which a test asserts alongside the removal.
- **220 tests** pass, clippy `-D warnings` clean. `cargo fmt --check` still reports 15 diffs in `alerts.rs`, `db.rs`, `legal.rs`, `legal_tests.rs`, `main.rs`, `telegram.rs`, `telegram_tests.rs` — all of them **pre-existing** (they reproduce with this change stashed), so they were left alone rather than mixed into this fix.
- **Deployed and the stored labels were corrected on production.** GitOps redeployed `acd1275` (`/api/app-info` = 0.4.3, container `4aed1c083cbd` healthy), then `POST /api/admin/classify/reclassify` returned `labels_written: 32`. Measured on the 100 stored articles, before and after:

  | | before | after |
  |---|---|---|
  | distinct entities | 147 | **10** |
  | articles carrying an entity | 100 | **12** |
  | `TICKER:LAW` | 100 | **0** |
  | `TICKER:ACT` | 97 | **0** |

  What survives are genuine statute acronyms (`FISA`, `LACA`, `GI`, `VA`, `ROAD`, `FAST`, `VETS`, `ALS`) and two `ORG:` matches. Topics are unchanged (`politics` 100/100) and every article keeps its original `govinfo.gov/app/details/...` link.
- **Operational note:** the redeploy looked wedged for over an hour — `PUT /git/redeploy` and `POST /stop` both returned 409 "Stack deployment is already in progress", `lastSyncDate` was 0 on two workflows, and no compiler had run for forty minutes. It recovered on its own once that sync cycle completed; **Portainer was not restarted and nothing was torn down.** `POST /api/admin/classify/reclassify` is the way to apply a rule change to articles already stored — fetching alone would never have fixed them.

## Plan-quality gate (cortex_plan_quality)

**2026-11-11, filings + Hub work**: three runs scored **6.9 → 7.4 → 7.0 / 10** (threshold 8.0) — not passed. Consistently lowest: **Impact Clarity 5–6**, **Testability 6**. Even a run that listed the before/after screen changes, exact file list, five named tests and the command exit codes scored Impact Clarity 5. The plan for this work is fully implemented and 159 tests pass; the score reflects the gate's judgement of the write-up, not missing code. No score was invented.

## Plan-quality gate (cortex_plan_quality) — earlier

After rewriting plan sections 5 and 8 (measurable acceptance, dependencies, rollback, decided-vs-open-vs-proposed), the gate was scored three times on 2026-11-11: **7.3 → 7.1 → 6.6 / 10** (threshold 8.0). It did not pass. Per the instruction, the work stopped there rather than rewording to chase a score.

Consistently lowest criteria across every run, including P1.5/P2/P3 attempts: **Impact Clarity 5/10**, **Risk Awareness 6/10**, **Testability 6/10**. The gate keeps asking for the same three things even when they are present in the submitted text ("clarify what will change / which files", "identify risks and rollback", "add verification steps"); the highest run (7.7 for P2) came from a requirement-to-deliverable table. That pattern suggests the tool weights structure it can recognise differently from the prose used here, not that the plan lacks those sections — the plan now contains 9 "Nghiệm thu đo được" sections, 8 rollback sections, an 11-row phase table with a dependency column, and section 8 split into decided / open / proposed.

No score was invented. If the gate matters, the next step is a human read of `.docs/plan.md` sections 5 and 8 rather than more rewording.

### P5 foundation (rule classification + LLM layer, offline)

- **Rules** (`src/classify.rs`, migration 13): topic by keyword (declared order breaks ties), region refined from place names only when the source is `global`, entities by pattern (`TICKER`, `EXCHANGE:SYMBOL`, `CRYPTO`, `CURRENCY`, `ORG`). Labels go to `article_matches` with their origin (`rule`/`llm`/`human`); a human correction is never overwritten and the superseded rule label is removed, so one article cannot show two contradictory topics. New articles are labelled during collection in the same transaction, and a labelling failure never loses the article.
- **LLM layer** (`src/llm.rs`): off by default; needs `LLM_BASE_URL` plus the `llm-api-key` secret; calls go through `fetch::Fetcher` (SSRF, size, redirect checks), the bearer token never follows a redirect, a UTC-daily token budget is checked **before** each request, and answers are cached by hash of model + prompt version + title. Any failure keeps the rule labels. Admin: `GET/PATCH /api/admin/llm` (status never exposes the key or the URL), `POST /api/admin/classify`, `POST /api/admin/classify/reclassify`.
- **Measured**: 50 hand-built items split into **25 dev** (used to tune keywords) and **25 holdout** (never used for tuning): **dev 24/25 = 96%, holdout 21/25 = 84%** against the 80% threshold. The test prints every miss. Reporting dev alone would have overstated the result; the holdout number is the one that counts.
- **Fixed during this work**: a real deadlock — the LLM status handler held the database lock and then called a function that locked it again.
- **Not done**: measuring against a real LLM (owner must pick Agent Hub vs LiteLLM and a budget), summaries, story grouping, and surfacing labels in the reader API/UI.

### P4 filings foundation + Hub provider layer (11/11/2026, offline)

- **Filings store** (migration 14): `companies`, `filings` (form type, accession, period, filed date, kind, original link, our summary + investment disclaimer) and `financial_facts` (normalised metric, provider concept, period, unit, currency, `as_of`, revision). Only metadata and structured numbers are stored — a test asserts no filing body or summary is written by the connector.
- **EDGAR connector** `sec-edgar-filings`: submissions API + XBRL companyfacts, two requests per company, disabled by default, requires the owner's `SEC_USER_AGENT`. Two real bugs were found by its tests: the undocumented wrapper `filings.recent` was missing, and the accession/date fields are camelCase (`accessionNumber`, `filingDate`, `reportDate`, `primaryDocument`). Unknown XBRL concepts are dropped rather than stored under a guessed metric.
- **Provider layer** (`src/hub.rs`): one OpenAI-compatible provider for both LLM tasks (per-task model aliases) and Hub agents (`/agents/{id}/run`, with timeout and output cap). The Hub key is a write-only secret (`hub-api-key`); the base URL and model aliases are environment configuration. A daily token budget is checked before each call, answers are cached by hash of model + task + prompt + content, and every failure falls back to rule labels. **The host allowlist is per exact host name** — a private address stays blocked unless the admin names that host, and the permission is attached only to the Hub fetcher.
- **"Check connection"** calls `GET /models` only, runs only when the admin presses it, and bills no tokens.
- **Source terms** written to `.docs/sources-candidates.md` section 7 with A/B/C confidence: EDGAR A (10 req/s, contact User-Agent, reuse with citation); AMF B/C (no public API found); ESAP not live until July 2027; Vietnam C (no official API documented; sites failed on automated access).
- **Not verified**: the real Hub has never been contacted, and the real route names are unconfirmed (they are three constants in `src/hub.rs`). No key was created or entered by this work.

## Verification

- `cargo fmt --all -- --check`, `cargo clippy --offline --all-targets -- -D warnings` and `cargo test --offline` (159 passed) — pass. The new tests cover the tree and inheritance, FR including EU, a new nested folder (`asia/sg`) being detected but disabled, each pack failure class, keeping the previous configuration after a failed reload, symlink skipping, the reader region filter, the scheduler skipping disabled regions, admin-only region endpoints, the pack-source edit restriction, the migration preserving P0/P1 rows including legacy free-text regions, the fallback of a source pointing at an unknown region, `/api/v1/regions` exposing only enabled regions and no pack issues, dashboard counters equalling the region-filtered list (including disabled topics and closing a parent region), and the `/api/dashboard` region query being case-insensitive but still refusing disabled regions, the fetch-safety fixes above, secret round-trip/nonce/name-binding/rewrap-failure/API-leak checks, series revision and as-of semantics, every connector gate, and each provider connector against its own loopback mock (mempool.space, ECB SDMX, Federal Register, FRED, CoinGecko, SEC EDGAR), the P3 pack parsers, tax-deadline engine (weekend rules, short months, leap year, timezones, year rollover), folded FTS search, the disclaimer invariant and the rule toggle, and the P5 rule classifier on a dev/holdout split plus the LLM budget, cache, redirect-token and deadlock regressions, filings idempotence and restatement revisions, the EDGAR connector against a documented-shape mock, and the Hub provider (config/allowlist per host, budget, cache, agent cap, model listing, status not leaking the key). All use temporary directories, an in-memory database and loopback servers; no external network.
- `npm --prefix ui run build` — pass.
- Browser smoke on a loopback preview server (`127.0.0.1`, demo key, isolated temp database, build outside the repo because a stale debug `lnc-newsman.exe` from an earlier session holds the in-repo one): enabled EU then France, the effective set became `eu, fr, global`, the header picker listed only enabled regions and selecting Pháp persisted; `region=fr` returned 200 and `region=us` returned 400 while US was off; adding `asia/sg` plus a broken and a mismatched folder produced `sg` disabled under `asia`, two issues in the pack panel and no lost regions; an enabled pack source accepted only the enable toggle and rejected a rename; a hand-added source was refused in the disabled `us` region and accepted in `fr`. Paper/Sepia/Night and 375×812 showed no horizontal scroll. No feed was fetched; the only network was loopback.
- The main local database `.local-data` was copied to `.local-data.backup-pre-p15-*` before work; it has not been migrated or opened by this build. The preview used a separate temporary database.

## Known boundaries

- P1 acceptance (≥20 real RSS sources, dedupe and auto-pause observed on real data) needs the owner's approval to fetch live feeds and a source list. P1.5 acceptance likewise needs a vetted source list per region; every built-in source is a placeholder.
- Only rung 1 of the fetch ladder (RSS/Atom) exists. Direct HTML fetching with robots.txt handling, `lnc-scrapling`, proxies and agent research are not implemented. Feeds are assumed UTF-8.
- Region packs: only `region.toml` and `sources.toml` are read; `official.toml`, `tax.toml`, `markets.toml` and `macro.toml` belong to P2–P4 and are ignored. Renaming an existing region's folder or moving it to another parent is reported as a conflict instead of silently reparenting; the owner must handle it manually. There is no UI for deleting a region, and a directory removed from the pack keeps its row (by design, so a typo cannot wipe configuration).
- Region codes are folder names only (ASCII lowercase, digits, hyphens); display names come from `region.toml`.
- Dates without a timezone are treated as UTC. Article retention/cleanup is not implemented.
- Three-tier configuration, trusted-proxy rate limiting, audit trail and production security review remain open. The secret store exists but **nothing consumes it yet** (no provider): rotation and rewrap are tested but have not been exercised against real keys.
- Connector scheduling is not automatic: an admin presses run (`POST /api/admin/connectors/{id}/run`). No chart, no alerting on connector health, and no provider quota has been verified against a real service.
- P3 deadlines must be verified against each jurisdiction's law by the owner before any reminder is trusted; the sample rules and their notes say so explicitly.
- Live acceptance for all six providers is unrun. The terms cited in code and in the acceptance script were read from secondary sources for ECB reuse and from official pages for the rest; they must be re-read by the owner immediately before approving the live run.
- The outbound deny-list is a deny-list, not an allow-list; a public address of the operator's own infrastructure is still reachable.
- Icon workflow, port reservation (`8462` proposal), NAS deployment and the plan-quality gate remain open as before.

See [task.md](task.md), [plan.md](plan.md), [README](../README.md) and [API documentation](../API_DOCS.md).

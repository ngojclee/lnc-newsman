# LNC Newsman — Agent Guide

## Purpose

LNC Newsman is a human-first internal news reader with source curation, topic classification, LLM-assisted briefs, alerts, reports and a REST/MCP surface for other agents.

## Current phase

P1 collector plus P1.5 regions, local only. RSS/Atom feeds are fetched by `src/fetch.rs` (public-address check after DNS, pinned connection, per-hop redirect validation, size/time limits) and parsed by `src/feed.rs`, which keeps metadata only. The scheduler in `src/collector.rs` runs only when the `collector_enabled` setting is on (default off) and only for sources whose region is enabled or an ancestor of an enabled region. Regions and packs live in `src/regions.rs`: the pack root is the virtual region `global`, directory nesting is the tree, and a scan only registers regions/sources **disabled** — never enable anything automatically and never fetch during a scan. P1/P1.5 acceptance against real feeds, LLM, alerts and MCP are not done. Never build a `Fetcher` that allows private addresses outside tests; route every outbound fetch through `fetch::Fetcher`.

## Boundaries

- Do not deploy to Portainer, modify DNS, register a Business MCP connector or publish an image without explicit owner approval.
- Do not put tokens, cookies, source credentials, runtime databases or personal data in git.
- Store runtime data under `/data`; local development may use `./data`.
- Preserve source links and attribution; store clean feed excerpts and extracted readable article text in the runtime SQLite database (`/data`) for in-app Reader mode, FTS5 search and grounded AI analysis (never commit runtime data to git).
- `D:\_Finance` is a read-only research reference unless a specific import task is approved.

## Commands

```bash
# Backend
~/.cargo/bin/cargo.exe check
~/.cargo/bin/cargo.exe test

# UI
cd ui
npm ci
npm run build

# Local smoke
MANAGEMENT_KEY=dev-only-key BIND_ADDRESS=127.0.0.1 COOKIE_SECURE=false DATA_DIR=./data PORT=8000 cargo run
```

The production image builds the UI in a Node stage and serves it from the Rust process. `ui/dist/` is generated and ignored.

## Architecture rules

- Rust `axum` owns HTTP, auth, settings and SQLite; keep vendor/fetch engines behind modules.
- UI uses the reader shell for human pages and `/admin/#slug` for management pages.
- P0 management/API keys are environment-managed; encrypted runtime storage and bootstrap-to-runtime ownership are not implemented. Never echo secrets in status or inventory.
- P0 API keys have read-only endpoint access; management settings require a signed, revocable session cookie. Write requests require the documented CSRF header.
- Add a migration for every database schema change and update `API_DOCS.md` for contract changes.
- Provider keys go only through `src/secrets.rs` (`LNC_MASTER_KEY`). Never log, echo, return or render a secret value; the admin API is metadata-only by design. A connector must use the injected `fetch::Fetcher` and never build its own HTTP client. Provider connectors live in `src/providers/`; they are registered disabled and must keep using the injected fetcher. Never create an account, enter a key, or choose a contact address for the owner. A live run is allowed only after the owner approves `.docs/live-acceptance.md`.
- A region pack scan must stay offline and non-destructive: no deletion of regions/sources, no enabling, no network. Keep every pack validation issue visible in the UI instead of failing silently.
- Tax deadlines are **declared rules**, never derived from law by us: keep `verified = false` on any rule the owner has not checked with an authority, and never drop the legal/tax disclaimer that the server attaches to written summaries and to the calendar. Public holidays are not modelled; do not claim otherwise.

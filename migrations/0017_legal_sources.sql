-- P8 foundation: legal-text sources per region pack, with the terms gate that decides
-- whether Newsman may poll them at all.
--
-- The gate is data, not code, because the answer changes: a publisher can publish
-- reuse terms tomorrow, or withdraw them. Each row records what was actually read
-- (quote + date + URL) and the verdict that followed. `terms_verdict` is one of:
--
--   verified          terms read on the publisher's own page and they allow a small
--                     number of polling requests per day;
--   unverified        no published terms could be read, so polling is refused;
--   prohibited        terms explicitly forbid automated retrieval;
--   not_required      a licence grants reuse and nothing is needed beyond credit.
--
-- `enabled` is the owner's switch. It can only be turned on for `verified` or
-- `not_required` rows — the same "detection is not activation" rule as feeds, plus a
-- legal gate feeds do not have.
CREATE TABLE IF NOT EXISTS legal_sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pack_key TEXT NOT NULL UNIQUE,
  region TEXT NOT NULL REFERENCES regions(code),
  code TEXT NOT NULL,
  name_vi TEXT NOT NULL,
  name_en TEXT NOT NULL,
  -- 'feed' when the publisher announces new texts, 'api' for a documented endpoint.
  access_kind TEXT NOT NULL DEFAULT 'feed',
  -- Documented endpoint. Empty while the publisher documents none.
  endpoint TEXT NOT NULL DEFAULT '',
  -- Human-readable access rule, e.g. "RSS per collection, keyless".
  access_note TEXT NOT NULL DEFAULT '',
  licence TEXT NOT NULL DEFAULT '',
  attribution TEXT NOT NULL DEFAULT '',
  -- Legal verdict + the evidence it rests on. Evidence is what makes the gate
  -- auditable months later instead of a remembered opinion.
  terms_verdict TEXT NOT NULL DEFAULT 'unverified',
  terms_checked_on TEXT NOT NULL DEFAULT '',
  terms_url TEXT NOT NULL DEFAULT '',
  terms_quote TEXT NOT NULL DEFAULT '',
  terms_note TEXT NOT NULL DEFAULT '',
  -- Rate limit the publisher states, if any. 0 = not stated.
  rate_limit_per_day INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CHECK (terms_verdict IN ('verified', 'unverified', 'prohibited', 'not_required'))
);
CREATE INDEX IF NOT EXISTS legal_sources_region_idx ON legal_sources(region);

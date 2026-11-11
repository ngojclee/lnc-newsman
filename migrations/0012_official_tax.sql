-- P3 foundation: issuing authorities and tax-deadline sources declared by a region pack.
-- Every row starts disabled: discovery is not activation, exactly as with feeds.
CREATE TABLE IF NOT EXISTS official_bodies (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pack_key TEXT NOT NULL UNIQUE,
  region TEXT NOT NULL REFERENCES regions(code),
  code TEXT NOT NULL,
  name_vi TEXT NOT NULL,
  name_en TEXT NOT NULL,
  url TEXT NOT NULL,
  language TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 0,
  attribution TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS official_bodies_region_idx ON official_bodies(region);

CREATE TABLE IF NOT EXISTS tax_sources (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pack_key TEXT NOT NULL UNIQUE,
  region TEXT NOT NULL REFERENCES regions(code),
  code TEXT NOT NULL,
  name_vi TEXT NOT NULL,
  name_en TEXT NOT NULL,
  url TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'authority',
  enabled INTEGER NOT NULL DEFAULT 0,
  attribution TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS tax_sources_region_idx ON tax_sources(region);

-- Declared recurring deadlines. A pack row is a *rule*, not a legal fact: the
-- `verified` flag records whether the owner has checked it against the authority.
CREATE TABLE IF NOT EXISTS tax_deadlines (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pack_key TEXT NOT NULL UNIQUE,
  region TEXT NOT NULL REFERENCES regions(code),
  source_code TEXT NOT NULL,
  code TEXT NOT NULL,
  name_vi TEXT NOT NULL,
  name_en TEXT NOT NULL,
  cadence TEXT NOT NULL,
  month INTEGER,
  day INTEGER,
  -- "next_business_day" moves a weekend date forward; "previous_business_day" back;
  -- "none" keeps the calendar date.
  weekend_rule TEXT NOT NULL DEFAULT 'none',
  lead_days INTEGER NOT NULL DEFAULT 14,
  url TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  -- 0 = sample data that has NOT been checked with the tax authority.
  verified INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS tax_deadlines_region_idx ON tax_deadlines(region);

-- Written summaries carry a legal disclaimer by construction; the DB stores the flag
-- so an API can never lose it.
ALTER TABLE documents ADD COLUMN topic TEXT NOT NULL DEFAULT '';
ALTER TABLE documents ADD COLUMN disclaimer TEXT NOT NULL DEFAULT '';

-- Full-text search over documents, with a Vietnamese-diacritic-free copy of the text
-- so "thue" finds "thuế". FTS5 is a standard SQLite feature available in the bundled
-- build; the index is contentless (we store the folded text ourselves) so a rebuild is
-- always possible from `documents`.
CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
  provider UNINDEXED,
  number UNINDEXED,
  title,
  agency,
  summary,
  tokenize = 'unicode61 remove_diacritics 2'
);

-- P2 provider connectors. Every displayed value carries the attribution the provider's
-- terms require (for example "Source: ECB statistics"), and documents keep only
-- metadata: identifier, title, effective date and the original link. Summaries in this
-- table are written by us, never copied from the publisher.
ALTER TABLE series ADD COLUMN attribution TEXT NOT NULL DEFAULT '';
ALTER TABLE series_events ADD COLUMN attribution TEXT NOT NULL DEFAULT '';

CREATE TABLE IF NOT EXISTS documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  region TEXT NOT NULL DEFAULT 'global' REFERENCES regions(code),
  -- 'rule' | 'proposed_rule' | 'notice' | 'filing' | 'law'
  doc_type TEXT NOT NULL DEFAULT 'notice',
  -- Agency or issuing body as the provider states it.
  agency TEXT NOT NULL DEFAULT '',
  -- Official identifier, e.g. a Federal Register document number.
  number TEXT NOT NULL,
  title TEXT NOT NULL,
  -- The date the document takes effect, not the date it was published.
  effective_on TEXT,
  published_on TEXT,
  url TEXT NOT NULL,
  -- Our own wording only. NULL until a summary is written; the publisher's abstract is
  -- never copied here.
  summary TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS documents_provider_number_idx
  ON documents(provider, number);
CREATE INDEX IF NOT EXISTS documents_region_effective_idx
  ON documents(region, effective_on);

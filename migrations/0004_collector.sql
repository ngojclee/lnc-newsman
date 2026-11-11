-- P1 collector state. Articles keep metadata only: title, URL, identifier and dates.
ALTER TABLE sources ADD COLUMN etag TEXT;
ALTER TABLE sources ADD COLUMN last_modified TEXT;
ALTER TABLE sources ADD COLUMN last_success_at TEXT;
ALTER TABLE sources ADD COLUMN last_error TEXT;
ALTER TABLE sources ADD COLUMN consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sources ADD COLUMN next_fetch_at INTEGER;
ALTER TABLE sources ADD COLUMN paused_reason TEXT;
ALTER TABLE articles ADD COLUMN guid TEXT;
ALTER TABLE articles ADD COLUMN topic_slug TEXT REFERENCES topics(slug);
ALTER TABLE articles ADD COLUMN fetched_at INTEGER;
CREATE INDEX IF NOT EXISTS articles_published_idx ON articles(published_at);
CREATE INDEX IF NOT EXISTS articles_fetched_idx ON articles(fetched_at);
CREATE INDEX IF NOT EXISTS articles_topic_idx ON articles(topic_slug);
CREATE UNIQUE INDEX IF NOT EXISTS articles_source_guid_idx ON articles(source_id, guid) WHERE guid IS NOT NULL;
CREATE TABLE IF NOT EXISTS fetch_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_id INTEGER NOT NULL REFERENCES sources(id),
  started_at INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  items_seen INTEGER NOT NULL DEFAULT 0,
  items_new INTEGER NOT NULL DEFAULT 0,
  error TEXT
);
CREATE INDEX IF NOT EXISTS fetch_runs_source_idx ON fetch_runs(source_id, id);

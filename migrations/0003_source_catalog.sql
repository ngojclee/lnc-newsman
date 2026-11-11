-- Catalog metadata only. Adding a source does not issue any network requests.
ALTER TABLE sources ADD COLUMN language TEXT NOT NULL DEFAULT '';
ALTER TABLE sources ADD COLUMN region TEXT NOT NULL DEFAULT '';
ALTER TABLE sources ADD COLUMN topic_slug TEXT REFERENCES topics(slug);
ALTER TABLE sources ADD COLUMN fetch_interval_minutes INTEGER NOT NULL DEFAULT 60;
CREATE INDEX IF NOT EXISTS sources_topic_slug_idx ON sources(topic_slug);

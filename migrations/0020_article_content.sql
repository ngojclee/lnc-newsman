-- Full-content archive, reader mode & FTS5 search for articles (v0.7.0).
-- `summary` (from 0001_init) holds the short excerpt/lead; `content_text` holds the
-- clean readable body extracted from the feed (`content:encoded` / `<content>`) or
-- fetched on-demand / in the background from the canonical article URL.
ALTER TABLE articles ADD COLUMN content_text TEXT;
ALTER TABLE articles ADD COLUMN content_source TEXT NOT NULL DEFAULT 'none';
ALTER TABLE articles ADD COLUMN content_fetched_at INTEGER;
ALTER TABLE articles ADD COLUMN content_error TEXT;
ALTER TABLE articles ADD COLUMN word_count INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS articles_content_source_idx ON articles(content_source, id);

CREATE VIRTUAL TABLE IF NOT EXISTS articles_fts USING fts5(
  article_id UNINDEXED,
  title,
  summary,
  content_text,
  tokenize = 'unicode61 remove_diacritics 2'
);

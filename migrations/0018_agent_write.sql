-- P8 write surface: notes, analyses and article flags authored by a delegated Hub
-- agent. Kept in their own tables so a write can never touch an article, a source, a
-- region, a connector or a secret — the only rows an agent can create are the ones it
-- obviously owns.
--
-- `author_client` is the Hub client id taken from the *verified* delegation, never from
-- a tool argument: it is the label the owner sees in the UI to tell an agent's work
-- from their own. Rows an owner creates in the web UI leave it NULL.

CREATE TABLE IF NOT EXISTS newsman_notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  -- Exactly one of these: a note sits on an article, or on an official document.
  article_id INTEGER REFERENCES articles(id) ON DELETE CASCADE,
  document_id INTEGER REFERENCES documents(id) ON DELETE CASCADE,
  body TEXT NOT NULL,
  -- Hub client id, or NULL for a note written by the owner in the web UI.
  author_client TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK ((article_id IS NOT NULL) <> (document_id IS NOT NULL))
);
CREATE INDEX IF NOT EXISTS newsman_notes_article ON newsman_notes(article_id);
CREATE INDEX IF NOT EXISTS newsman_notes_document ON newsman_notes(document_id);

CREATE TABLE IF NOT EXISTS newsman_analyses (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL DEFAULT 'brief' CHECK (kind IN ('brief', 'summary')),
  title TEXT NOT NULL,
  -- A brief is scoped to a topic and/or a region; either may be null for a topical
  -- note that spans regions.
  topic_slug TEXT REFERENCES topics(slug),
  region_code TEXT REFERENCES regions(code),
  body TEXT NOT NULL,
  -- JSON array of article ids the writer used, so a claim can always be traced back
  -- to a stored item. Validated to be a JSON array of integers before insert.
  source_article_ids TEXT NOT NULL DEFAULT '[]',
  author_client TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS newsman_analyses_topic ON newsman_analyses(topic_slug);
CREATE INDEX IF NOT EXISTS newsman_analyses_region ON newsman_analyses(region_code);
CREATE INDEX IF NOT EXISTS newsman_analyses_updated ON newsman_analyses(updated_at DESC);

-- A flag is a per-article marker an agent sets ("needs analysis", "used in brief X").
-- One flag per (article, label) so repeating it is idempotent rather than duplicating.
CREATE TABLE IF NOT EXISTS newsman_article_flags (
  article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
  label TEXT NOT NULL,
  author_client TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (article_id, label)
);

-- One row per accepted write. `client_id` is the verified delegation caller; `tool`
-- and `record_id` say what was touched. This is the audit trail the owner can read and
-- an agent cannot alter: no tool inserts here directly, only the write path does.
CREATE TABLE IF NOT EXISTS newsman_write_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  client_id TEXT,
  app_id TEXT,
  tool TEXT NOT NULL,
  record_kind TEXT NOT NULL,
  record_id INTEGER,
  outcome TEXT NOT NULL,
  note TEXT
);
CREATE INDEX IF NOT EXISTS newsman_write_log_at ON newsman_write_log(at DESC);

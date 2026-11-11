-- P5 foundation: rule-based classification, entity extraction and an LLM layer.
-- `article_matches` stores what a classifier decided, with the rule or model that
-- decided it, so a human can audit or correct any label.
CREATE TABLE IF NOT EXISTS article_matches (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  article_id INTEGER NOT NULL REFERENCES articles(id),
  -- 'region' | 'topic' | 'entity'
  kind TEXT NOT NULL,
  value TEXT NOT NULL,
  -- 'rule' | 'llm' | 'human'
  origin TEXT NOT NULL,
  confidence REAL NOT NULL DEFAULT 1.0,
  -- Which rule fired, or which model version answered.
  reason TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(article_id, kind, value)
);
CREATE INDEX IF NOT EXISTS article_matches_kind_idx ON article_matches(kind, value);

-- Token budget per UTC day and cache for LLM answers keyed by content hash.
CREATE TABLE IF NOT EXISTS llm_usage (
  day TEXT PRIMARY KEY,
  tokens_used INTEGER NOT NULL DEFAULT 0,
  calls INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS llm_cache (
  -- sha256(model_version + prompt_version + content hash)
  cache_key TEXT PRIMARY KEY,
  answer TEXT NOT NULL,
  tokens INTEGER NOT NULL DEFAULT 0,
  model TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

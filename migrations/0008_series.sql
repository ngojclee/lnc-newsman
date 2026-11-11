-- P2 foundation: time series, observations and events (no data provider is wired yet).
-- An observation separates when a value was *observed* (`observed_at`, the effective
-- time) from when we *learned* it (`as_of`), and every correction is a new revision
-- instead of overwriting history, so an as-of query can reproduce what was known then.
CREATE TABLE IF NOT EXISTS series (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  code TEXT NOT NULL UNIQUE,
  provider TEXT NOT NULL,
  region TEXT NOT NULL DEFAULT 'global' REFERENCES regions(code),
  label_vi TEXT NOT NULL,
  label_en TEXT NOT NULL,
  unit TEXT NOT NULL DEFAULT '',
  frequency TEXT NOT NULL DEFAULT 'daily',
  enabled INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS series_region_idx ON series(region);
CREATE INDEX IF NOT EXISTS series_provider_idx ON series(provider);

CREATE TABLE IF NOT EXISTS observations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  series_id INTEGER NOT NULL REFERENCES series(id),
  -- Effective time of the value (the date the number describes), Unix seconds.
  observed_at INTEGER NOT NULL,
  -- When this revision became known, Unix seconds. Corrections are later as_of values.
  as_of INTEGER NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  value REAL NOT NULL,
  -- Where it came from, for auditability. Never a place to store provider keys.
  source_url TEXT,
  source_ref TEXT,
  -- 'preliminary' | 'final' | 'revised'
  quality TEXT NOT NULL DEFAULT 'final',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
-- One row per (series, period, revision); a correction adds a revision, never overwrites.
CREATE UNIQUE INDEX IF NOT EXISTS observations_revision_idx
  ON observations(series_id, observed_at, revision);
CREATE INDEX IF NOT EXISTS observations_asof_idx ON observations(series_id, as_of DESC);
CREATE INDEX IF NOT EXISTS observations_time_idx ON observations(observed_at DESC);

CREATE TABLE IF NOT EXISTS series_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  provider TEXT NOT NULL,
  region TEXT NOT NULL DEFAULT 'global' REFERENCES regions(code),
  series_code TEXT,
  kind TEXT NOT NULL DEFAULT 'release',
  title TEXT NOT NULL,
  url TEXT,
  starts_at INTEGER NOT NULL,
  importance INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX IF NOT EXISTS series_events_unique_idx
  ON series_events(provider, starts_at, title);
CREATE INDEX IF NOT EXISTS series_events_time_idx ON series_events(starts_at);

-- Connector bookkeeping: quota, schedule, cache and health per provider/region.
CREATE TABLE IF NOT EXISTS connectors (
  id TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  region TEXT NOT NULL DEFAULT 'global' REFERENCES regions(code),
  enabled INTEGER NOT NULL DEFAULT 0,
  -- Provider credentials are referenced by name only; the value lives in `secrets`.
  secret_name TEXT,
  min_interval_seconds INTEGER NOT NULL DEFAULT 3600,
  -- Budget per UTC day, so a misconfigured schedule cannot drain a provider quota.
  daily_quota INTEGER NOT NULL DEFAULT 1000,
  cache_seconds INTEGER NOT NULL DEFAULT 900,
  last_run_at INTEGER,
  next_run_at INTEGER,
  last_success_at INTEGER,
  last_error TEXT,
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  requests_today INTEGER NOT NULL DEFAULT 0,
  quota_day TEXT,
  health TEXT NOT NULL DEFAULT 'unknown',
  candidates_checked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS connectors_next_run_idx ON connectors(enabled, next_run_at);

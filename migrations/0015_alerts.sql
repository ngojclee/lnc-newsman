-- P7 foundation: alert rules, deliveries and the in-app notification inbox.
--
-- A rule is a declaration, like a tax deadline rule: it fires when a new article
-- matches, at most `cooldown_seconds` after its last firing, and never during its
-- quiet hours. Every firing is recorded once per (rule, match) so re-running the
-- sweep cannot deliver the same event twice.
CREATE TABLE IF NOT EXISTS alert_rules (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  slug TEXT NOT NULL UNIQUE,
  label_vi TEXT NOT NULL,
  label_en TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0,
  -- 'region' | 'topic' | 'entity' | 'keyword'
  match_kind TEXT NOT NULL,
  match_value TEXT NOT NULL,
  -- 'info' | 'important' | 'urgent'
  level TEXT NOT NULL DEFAULT 'info',
  cooldown_seconds INTEGER NOT NULL DEFAULT 21600,
  -- Local time in the rule's timezone, HH:MM inclusive start / exclusive end.
  quiet_from TEXT NOT NULL DEFAULT '',
  quiet_to TEXT NOT NULL DEFAULT '',
  -- IANA zone, e.g. Asia/Ho_Chi_Minh. Empty means UTC.
  timezone TEXT NOT NULL DEFAULT 'UTC',
  -- 'in-app' is the only channel implemented offline; the rest are declared but off.
  channels TEXT NOT NULL DEFAULT 'in-app',
  last_fired_at INTEGER,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- One row per (rule, article). UNIQUE is what makes "fires exactly once" a property of
-- the schema instead of something the sweep has to remember.
CREATE TABLE IF NOT EXISTS alert_deliveries (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  rule_id INTEGER NOT NULL REFERENCES alert_rules(id),
  article_id INTEGER NOT NULL REFERENCES articles(id),
  level TEXT NOT NULL,
  channel TEXT NOT NULL DEFAULT 'in-app',
  title TEXT NOT NULL,
  body TEXT NOT NULL DEFAULT '',
  -- 1 once a human has opened it; the inbox counts unread rows.
  read_at INTEGER,
  created_at INTEGER NOT NULL,
  UNIQUE(rule_id, article_id)
);
CREATE INDEX IF NOT EXISTS alert_deliveries_created_idx ON alert_deliveries(created_at DESC);
CREATE INDEX IF NOT EXISTS alert_deliveries_unread_idx ON alert_deliveries(read_at);

-- Why a rule did NOT fire, so the settings screen can explain silence instead of
-- leaving it ambiguous between "nothing matched" and "never ran".
CREATE TABLE IF NOT EXISTS alert_skip_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  rule_id INTEGER NOT NULL REFERENCES alert_rules(id),
  article_id INTEGER NOT NULL REFERENCES articles(id),
  reason TEXT NOT NULL,
  -- Same subject is skipped once; a later sweep may fire if the reason clears
  -- (quiet hours end, cooldown expires).
  created_at INTEGER NOT NULL,
  UNIQUE(rule_id, article_id)
);

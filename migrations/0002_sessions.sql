-- Store only hashes of signed random session tokens. Single-instance P0.
CREATE TABLE IF NOT EXISTS management_sessions (
  token_hash TEXT PRIMARY KEY,
  expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS management_sessions_expiry_idx ON management_sessions(expires_at);

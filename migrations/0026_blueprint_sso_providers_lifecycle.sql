-- LNC Internal App Blueprint v2: SSO/OIDC runtime auth, dual-envelope API keys,
-- unified AI provider hub, custom MCP connectors, and audit trail.

CREATE TABLE IF NOT EXISTS runtime_auth_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  mgmt_key_hash TEXT NOT NULL DEFAULT '',
  mgmt_source TEXT NOT NULL DEFAULT 'unset',
  bootstrap_env_digest TEXT NOT NULL DEFAULT '',
  session_epoch INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO runtime_auth_state (
  id, mgmt_key_hash, mgmt_source, bootstrap_env_digest, session_epoch, updated_at
) VALUES (1, '', 'unset', '', 1, 0);

ALTER TABLE management_sessions ADD COLUMN idle_expires_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE management_sessions ADD COLUMN max_expires_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE management_sessions ADD COLUMN session_epoch INTEGER NOT NULL DEFAULT 1;
ALTER TABLE management_sessions ADD COLUMN auth_method TEXT NOT NULL DEFAULT 'management_key';
ALTER TABLE management_sessions ADD COLUMN username TEXT NOT NULL DEFAULT '';
ALTER TABLE management_sessions ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;

UPDATE management_sessions
SET
  idle_expires_at = CASE WHEN idle_expires_at = 0 THEN expires_at ELSE idle_expires_at END,
  max_expires_at = CASE WHEN max_expires_at = 0 THEN expires_at ELSE max_expires_at END
WHERE idle_expires_at = 0 OR max_expires_at = 0;

CREATE TABLE IF NOT EXISTS api_keys (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  prefix TEXT NOT NULL DEFAULT 'nm_',
  last4 TEXT NOT NULL,
  key_hash_hmac TEXT NOT NULL UNIQUE,
  secret_enc TEXT NOT NULL DEFAULT '',
  copy_available INTEGER NOT NULL DEFAULT 0,
  scopes_json TEXT NOT NULL DEFAULT '["newsman:read"]',
  status TEXT NOT NULL DEFAULT 'active',
  source TEXT NOT NULL DEFAULT 'admin',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  last_used_at INTEGER NOT NULL DEFAULT 0,
  revoked_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS api_keys_status_idx ON api_keys(status, created_at DESC);

CREATE TABLE IF NOT EXISTS ai_providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'openai_compat',
  base_url TEXT NOT NULL DEFAULT '',
  secret_name TEXT NOT NULL DEFAULT '',
  secret_enc TEXT NOT NULL DEFAULT '',
  key_last4 TEXT NOT NULL DEFAULT '',
  copy_available INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'active',
  is_default INTEGER NOT NULL DEFAULT 0,
  capabilities_json TEXT NOT NULL DEFAULT '["text"]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  revoked_at TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO ai_providers (
  id, name, kind, base_url, secret_name, capabilities_json, enabled, status, is_default, created_at, updated_at
)
SELECT
  id, name, 'openai_compat', base_url, secret_name, capabilities_json, enabled, 'active', 0, created_at, updated_at
FROM app_agent_custom_providers;

CREATE TABLE IF NOT EXISTS ai_model_catalog (
  provider_id TEXT NOT NULL REFERENCES ai_providers(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  capabilities_json TEXT NOT NULL DEFAULT '["text"]',
  context_window INTEGER,
  enabled INTEGER NOT NULL DEFAULT 0,
  last_fetched_at TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (provider_id, model_id)
);

CREATE TABLE IF NOT EXISTS ai_consumer_bindings (
  role_key TEXT PRIMARY KEY,
  provider_id TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
  model_id TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS mcp_connectors (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  direction TEXT NOT NULL DEFAULT 'outbound',
  endpoint_url TEXT NOT NULL DEFAULT '',
  secret_name TEXT NOT NULL DEFAULT '',
  secret_enc TEXT NOT NULL DEFAULT '',
  key_last4 TEXT NOT NULL DEFAULT '',
  copy_available INTEGER NOT NULL DEFAULT 0,
  scopes_json TEXT NOT NULL DEFAULT '["newsman:read"]',
  audience TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'active',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  revoked_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS audit_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  at INTEGER NOT NULL,
  actor TEXT NOT NULL DEFAULT '',
  auth_method TEXT NOT NULL DEFAULT '',
  client_ip TEXT NOT NULL DEFAULT '',
  action TEXT NOT NULL,
  target_type TEXT NOT NULL DEFAULT '',
  target_id TEXT NOT NULL DEFAULT '',
  detail_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS audit_events_at_idx ON audit_events(at DESC, id DESC);
CREATE INDEX IF NOT EXISTS audit_events_target_idx ON audit_events(target_type, target_id, at DESC);

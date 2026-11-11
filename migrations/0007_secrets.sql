-- Encrypted provider secrets (P2 foundation). Values are XChaCha20-Poly1305
-- ciphertext only; the master key never touches the database. One nonce per write,
-- a fresh nonce per rewrap, and no API surface ever returns the plaintext.
CREATE TABLE IF NOT EXISTS secrets (
  name TEXT PRIMARY KEY,
  -- Base64 of nonce || ciphertext+tag, bound to `name` as associated data.
  ciphertext TEXT NOT NULL,
  -- Which master-key generation encrypted it, so rotation can rewrap in batches.
  key_version INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

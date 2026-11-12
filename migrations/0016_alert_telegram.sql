-- P7 Telegram channel: one outbox row per delivery that must leave the app.
--
-- The delivery row (alert_deliveries) is written inside the sweep's transaction and is
-- what makes "fires exactly once" true. Sending is network I/O and cannot run while the
-- database lock is held, so each delivery that asks for Telegram also gets one outbox
-- row here. The flusher claims a row (pending -> sending) before it calls the relay, so
-- two concurrent flushes cannot both send it, and a row left in `sending` by a crash is
-- reclaimed after a grace period.
CREATE TABLE IF NOT EXISTS alert_telegram_outbox (
  delivery_id INTEGER PRIMARY KEY REFERENCES alert_deliveries(id),
  -- 'pending' | 'sending' | 'sent' | 'failed'
  state TEXT NOT NULL DEFAULT 'pending',
  attempts INTEGER NOT NULL DEFAULT 0,
  -- HTTP/transport code from the last attempt; never the message or a token.
  last_error TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS alert_telegram_outbox_state_idx
  ON alert_telegram_outbox(state, created_at);

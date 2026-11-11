-- Every connector run must record how many requests it actually spent, so the daily
-- quota is enforced against real traffic instead of "one per run".
ALTER TABLE connectors ADD COLUMN requests_last_run INTEGER NOT NULL DEFAULT 0;

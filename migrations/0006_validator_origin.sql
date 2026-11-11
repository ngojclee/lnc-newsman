-- P1.5/A follow-up: remember which origin issued a source's ETag/Last-Modified.
-- Validators are only replayed to that origin (see fetch::Validators), so they must
-- survive a restart; a cross-origin redirect therefore cannot leak them onward.
-- Existing rows have no origin, so their validators are simply not replayed once and
-- the next successful run records the correct one.
ALTER TABLE sources ADD COLUMN validators_origin TEXT;

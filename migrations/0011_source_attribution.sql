-- Provider credit line for a source, when the publisher's terms require one.
-- Empty means the reader relies on the link back to the original.
ALTER TABLE sources ADD COLUMN attribution TEXT NOT NULL DEFAULT '';

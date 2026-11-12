-- A note may now hang off an analysis, not only off an article or a document.
-- This is what closes the Obsidian loop: the owner writes an opinion under a
-- forecast in their vault, and that opinion comes back as a note on the analysis.
--
-- SQLite cannot ALTER a CHECK constraint, so the table is rebuilt: create the new
-- shape, copy every row, drop the old, rename. The CHECK becomes "exactly one of
-- three targets". The foreign key is declared ON DELETE CASCADE, so deleting a
-- deleted analysis takes its notes with it — but foreign keys are only enforced
-- when `PRAGMA foreign_keys` is on, which is why the copy is explicit and ordered.
--
-- Migration 18 is untouched: a migration is a historical record, and editing one
-- would mean databases already past it never get this change.

CREATE TABLE newsman_notes_v19 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  article_id INTEGER REFERENCES articles(id) ON DELETE CASCADE,
  document_id INTEGER REFERENCES documents(id) ON DELETE CASCADE,
  analysis_id INTEGER REFERENCES newsman_analyses(id) ON DELETE CASCADE,
  body TEXT NOT NULL,
  author_client TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  -- Exactly one target, not zero and not two.
  CHECK (
    (article_id IS NOT NULL)
    + (document_id IS NOT NULL)
    + (analysis_id IS NOT NULL) = 1
  )
);

INSERT INTO newsman_notes_v19 (id, article_id, document_id, analysis_id, body, author_client, created_at, updated_at)
SELECT id, article_id, document_id, NULL, body, author_client, created_at, updated_at FROM newsman_notes;

DROP TABLE newsman_notes;

ALTER TABLE newsman_notes_v19 RENAME TO newsman_notes;

CREATE INDEX IF NOT EXISTS newsman_notes_article ON newsman_notes(article_id);
CREATE INDEX IF NOT EXISTS newsman_notes_document ON newsman_notes(document_id);
CREATE INDEX IF NOT EXISTS newsman_notes_analysis ON newsman_notes(analysis_id);

-- Reading what changed since a moment is the whole point of the Obsidian pull, so
-- it must not be a full scan of every analysis.
CREATE INDEX IF NOT EXISTS newsman_analyses_updated_idx ON newsman_analyses(updated_at);

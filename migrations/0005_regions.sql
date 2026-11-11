-- P1.5 region tree and region packs. Existing sources and articles are kept:
-- rows without a region are adopted into `global` when a pack is loaded, never deleted.
CREATE TABLE IF NOT EXISTS regions (
  code TEXT PRIMARY KEY,
  parent_code TEXT REFERENCES regions(code),
  label_vi TEXT NOT NULL,
  label_en TEXT NOT NULL,
  currency TEXT NOT NULL DEFAULT '',
  timezone TEXT NOT NULL DEFAULT '',
  language TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 0,
  builtin INTEGER NOT NULL DEFAULT 1,
  managed INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS regions_parent_idx ON regions(parent_code);

-- `global` is the virtual root: it always exists and can never be disabled.
INSERT OR IGNORE INTO regions(code,parent_code,label_vi,label_en,currency,timezone,language,enabled,builtin,managed)
VALUES ('global',NULL,'Toàn cầu','Global','','UTC','',1,1,1);

-- `sources.region` is reused as the region code. P1 allowed free text there, so any
-- value that is not a known region code (only `global` exists at this point) is reset
-- to `global`; otherwise those sources' items would silently leave the reader.
UPDATE sources SET region='global' WHERE region IS NULL OR region<>'global';
ALTER TABLE sources ADD COLUMN pack_key TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS sources_pack_key_idx ON sources(pack_key) WHERE pack_key IS NOT NULL;
ALTER TABLE articles ADD COLUMN region_code TEXT;
UPDATE articles SET region_code=COALESCE((SELECT s.region FROM sources s WHERE s.id=articles.source_id),'global') WHERE region_code IS NULL;
CREATE INDEX IF NOT EXISTS articles_region_idx ON articles(region_code);

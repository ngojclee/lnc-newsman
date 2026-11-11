-- P4 foundation: company filings and structured financial facts.
--
-- Only metadata + structured numbers + the original link are stored; a filing's full
-- text is never copied. A fact keeps the period it describes, the period it was
-- reported for, when we learned it and its revision, so a restatement is a new row
-- rather than a silent overwrite.
CREATE TABLE IF NOT EXISTS companies (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  -- Central Index Key or the provider's own id.
  external_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  region TEXT NOT NULL DEFAULT 'us' REFERENCES regions(code),
  name TEXT NOT NULL,
  ticker TEXT,
  exchange TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, external_id)
);

CREATE TABLE IF NOT EXISTS filings (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  company_id INTEGER NOT NULL REFERENCES companies(id),
  provider TEXT NOT NULL,
  -- 10-K, 10-Q, 8-K, S-1, ...
  form_type TEXT NOT NULL,
  -- Provider document id, e.g. EDGAR accession number.
  accession TEXT NOT NULL,
  -- Report period the filing covers (YYYY-MM-DD), when the provider states it.
  period_end TEXT,
  filed_on TEXT NOT NULL,
  -- 'annual' | 'quarterly' | 'current' | 'registration' | 'other'
  filing_kind TEXT NOT NULL DEFAULT 'other',
  url TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  -- Our own wording only. NULL until a human writes it.
  summary TEXT,
  disclaimer TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, accession)
);
CREATE INDEX IF NOT EXISTS filings_company_idx ON filings(company_id, filed_on DESC);
CREATE INDEX IF NOT EXISTS filings_kind_idx ON filings(filing_kind, filed_on DESC);

CREATE TABLE IF NOT EXISTS financial_facts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  company_id INTEGER NOT NULL REFERENCES companies(id),
  provider TEXT NOT NULL,
  -- Normalised metric key: revenue, net_income, total_assets, total_liabilities, ...
  metric TEXT NOT NULL,
  -- The provider's own concept, kept for auditability (e.g. us-gaap:Revenues).
  concept TEXT NOT NULL,
  -- Period the value describes.
  period_start TEXT,
  period_end TEXT NOT NULL,
  -- 'duration' (a quarter/year) or 'instant' (a balance-sheet date).
  period_kind TEXT NOT NULL DEFAULT 'duration',
  -- 'Q1'..'Q4' | 'FY' | 'H1' | ''
  fiscal_period TEXT NOT NULL DEFAULT '',
  fiscal_year INTEGER,
  value REAL NOT NULL,
  unit TEXT NOT NULL,
  -- Frozen FX is not applied here: values stay in the provider's currency.
  currency TEXT NOT NULL DEFAULT 'USD',
  -- When this revision became known.
  as_of INTEGER NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  -- 'preliminary' | 'final' | 'restated'
  quality TEXT NOT NULL DEFAULT 'final',
  source_url TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  -- One row per company, metric, period and revision.
  UNIQUE(company_id, metric, period_end, revision)
);
CREATE INDEX IF NOT EXISTS financial_facts_metric_idx ON financial_facts(company_id, metric, period_end DESC);

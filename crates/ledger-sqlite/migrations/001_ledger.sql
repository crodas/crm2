CREATE TABLE IF NOT EXISTS ledger_assets (
    name      TEXT PRIMARY KEY,
    precision INTEGER NOT NULL,
    kind      TEXT NOT NULL CHECK (kind IN ('signed', 'unsigned'))
);

CREATE TABLE IF NOT EXISTS ledger_transactions (
    rowid           INTEGER PRIMARY KEY,
    tx_id           TEXT NOT NULL UNIQUE,
    idempotency_key TEXT NOT NULL UNIQUE,
    data            TEXT NOT NULL  -- JSON-serialized Transaction
);

CREATE TABLE IF NOT EXISTS ledger_credit_tokens (
    tx_id       TEXT    NOT NULL,
    entry_index INTEGER NOT NULL,
    owner       TEXT    NOT NULL,
    asset_name  TEXT    NOT NULL,
    qty         INTEGER NOT NULL,
    spent_by_tx TEXT,
    PRIMARY KEY (tx_id, entry_index)
);

CREATE INDEX IF NOT EXISTS idx_ledger_credit_tokens_unspent_account
    ON ledger_credit_tokens (owner, asset_name) WHERE spent_by_tx IS NULL;

CREATE INDEX IF NOT EXISTS idx_ledger_credit_tokens_unspent_prefix
    ON ledger_credit_tokens (asset_name) WHERE spent_by_tx IS NULL;

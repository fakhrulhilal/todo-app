-- The PostgreSQL schema every implementation shares, as its desired state.
-- pgschema compares this file with the database and applies the difference;
-- compose.yaml does that on every `docker compose up` (service db-init).
--
-- Edit it in place. The apps migrate with the numbered files in migrations/
-- instead (database.migration), so a schema change also adds one there and
-- one in ../sqlite/, and all three must end at the same schema. See the
-- root README, "Changing the schema".

CREATE TABLE IF NOT EXISTS todos (
    id         uuid PRIMARY KEY,
    title      text NOT NULL,
    completed  boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL
);

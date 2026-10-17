CREATE TABLE todos (
    id         UUID PRIMARY KEY,
    title      TEXT NOT NULL,
    completed  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

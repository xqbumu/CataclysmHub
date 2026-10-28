CREATE TABLE play_instance (
    id TEXT PRIMARY KEY,
    installation_id TEXT NOT NULL REFERENCES installation(id),
    installation_sha256 TEXT NOT NULL,
    name TEXT NOT NULL,
    data_root TEXT NOT NULL UNIQUE,
    binding_status TEXT NOT NULL CHECK(binding_status = 'unverified'),
    created_at TEXT NOT NULL
);
PRAGMA user_version = 7;

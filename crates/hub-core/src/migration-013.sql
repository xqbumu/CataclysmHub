CREATE TABLE config_change (
 id TEXT PRIMARY KEY,
 request_key TEXT NOT NULL UNIQUE,
 request_digest TEXT NOT NULL,
 instance_id TEXT NOT NULL,
 backup_id TEXT NOT NULL,
 data_root TEXT NOT NULL,
 path TEXT NOT NULL CHECK(path IN ('config/options.json','config/user-default-mods.json')),
 before_sha256 TEXT NOT NULL,
 after_sha256 TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('applying','applied','recovery_required','resolved_original','resolved_proposed')),
 error_code TEXT,
 created_at TEXT NOT NULL,
 finished_at TEXT
);
PRAGMA user_version=13;

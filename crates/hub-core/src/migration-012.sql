CREATE TABLE content_install_task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL,
  entry_id TEXT NOT NULL,
  source_root TEXT NOT NULL,
  destination TEXT NOT NULL UNIQUE,
  payload_root TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);
PRAGMA user_version=12;

CREATE TABLE installation_copy_task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL,
  source_installation_id TEXT NOT NULL REFERENCES installation(id),
  source_root TEXT NOT NULL,
  parent_dir TEXT NOT NULL,
  destination TEXT NOT NULL UNIQUE,
  executable_relative TEXT NOT NULL,
  executable_sha256 TEXT NOT NULL,
  copy_id TEXT NOT NULL,
  payload_root TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL,
  entry_count INTEGER NOT NULL,
  file_count INTEGER NOT NULL,
  link_count INTEGER NOT NULL,
  total_bytes TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  processed_entries INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 1,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  verified_at TEXT,
  registered_installation_id TEXT REFERENCES installation(id)
);
CREATE INDEX installation_copy_created ON installation_copy_task(created_at DESC,id);
PRAGMA user_version=10;

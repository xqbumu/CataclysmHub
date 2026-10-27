CREATE TABLE backup_task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  backup_id TEXT NOT NULL,
  operation TEXT NOT NULL CHECK(operation IN ('backup.create','backup.verify')),
  state TEXT NOT NULL CHECK(state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  processed_entries INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 1,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);
CREATE INDEX backup_task_created ON backup_task(created_at DESC,id);
CREATE TABLE backup_archive (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL REFERENCES backup_task(id),
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  relative_dir TEXT NOT NULL UNIQUE,
  created_at TEXT NOT NULL,
  entry_count INTEGER NOT NULL,
  file_count INTEGER NOT NULL,
  total_bytes TEXT NOT NULL,
  consistency TEXT NOT NULL,
  verification TEXT NOT NULL,
  verified_at TEXT NOT NULL,
  archive_sha256 TEXT NOT NULL,
  manifest_sha256 TEXT NOT NULL
);
PRAGMA user_version = 3;

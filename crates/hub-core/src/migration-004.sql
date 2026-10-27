CREATE TABLE restore_task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL,
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  backup_id TEXT NOT NULL REFERENCES backup_archive(id),
  destination TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);
CREATE INDEX restore_task_created ON restore_task(created_at DESC,id);
PRAGMA user_version = 4;

CREATE TABLE inspection_task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  request_digest TEXT NOT NULL,
  restore_task_id TEXT NOT NULL REFERENCES restore_task(id),
  state TEXT NOT NULL CHECK(state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  revision INTEGER NOT NULL DEFAULT 1,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT,
  result_json TEXT CHECK(result_json IS NULL OR length(result_json) <= 65536)
);
CREATE INDEX inspection_task_created ON inspection_task(created_at DESC,id);
PRAGMA user_version = 5;

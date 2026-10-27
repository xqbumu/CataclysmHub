CREATE TABLE task (
  id TEXT PRIMARY KEY,
  request_key TEXT NOT NULL UNIQUE,
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  operation TEXT NOT NULL CHECK (operation = 'world.scan'),
  state TEXT NOT NULL CHECK (state IN ('running','cancelling','succeeded','failed','cancelled','recovery_required')),
  phase TEXT NOT NULL,
  processed_entries INTEGER NOT NULL DEFAULT 0,
  revision INTEGER NOT NULL DEFAULT 1,
  result_json TEXT,
  error_code TEXT,
  created_at TEXT NOT NULL,
  finished_at TEXT
);
CREATE INDEX task_created ON task(created_at DESC, id);
CREATE TABLE world_candidate (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspace(id),
  relative_root TEXT NOT NULL,
  alias TEXT,
  notes TEXT NOT NULL DEFAULT '',
  category TEXT NOT NULL DEFAULT 'unclassified',
  revision INTEGER NOT NULL DEFAULT 1,
  last_seen_task_id TEXT NOT NULL REFERENCES task(id),
  UNIQUE (workspace_id, relative_root)
);
PRAGMA user_version = 2;

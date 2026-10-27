CREATE TABLE workspace (
  id TEXT PRIMARY KEY,
  save_dir TEXT NOT NULL,
  backup_dir TEXT NOT NULL,
  content_dir TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE TABLE installation (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  executable TEXT NOT NULL UNIQUE,
  working_dir TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  version TEXT,
  branch TEXT,
  platform TEXT NOT NULL,
  arch TEXT NOT NULL,
  imported_at TEXT NOT NULL
);
CREATE TABLE session (
  id TEXT PRIMARY KEY,
  installation_id TEXT NOT NULL REFERENCES installation(id),
  state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'exited_unverified', 'failed', 'interrupted_unknown', 'resolved_unknown')),
  pid INTEGER,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  exit_code INTEGER
);
CREATE TABLE operation_log (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  action TEXT NOT NULL,
  status TEXT NOT NULL,
  error_code TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX session_created ON session(started_at DESC, id);
CREATE INDEX operation_created ON operation_log(created_at DESC, id);
PRAGMA user_version = 1;

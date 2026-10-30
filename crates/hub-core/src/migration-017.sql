CREATE TABLE automatic_backup_policy (
  instance_id TEXT PRIMARY KEY REFERENCES play_instance(id),
  revision INTEGER NOT NULL,
  settings TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE TABLE automatic_backup_run (
  session_id TEXT PRIMARY KEY REFERENCES session(id),
  instance_id TEXT NOT NULL REFERENCES play_instance(id),
  policy_revision INTEGER NOT NULL,
  task_id TEXT REFERENCES backup_task(id),
  status TEXT NOT NULL,
  error_code TEXT,
  created_at TEXT NOT NULL
);
PRAGMA user_version = 17;

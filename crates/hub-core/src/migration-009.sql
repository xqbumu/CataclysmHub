ALTER TABLE backup_task ADD COLUMN play_instance_id TEXT REFERENCES play_instance(id);
ALTER TABLE backup_task ADD COLUMN source_root TEXT;
ALTER TABLE backup_archive ADD COLUMN play_instance_id TEXT REFERENCES play_instance(id);
ALTER TABLE backup_archive ADD COLUMN source_root TEXT;
PRAGMA user_version = 9;

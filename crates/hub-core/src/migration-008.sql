ALTER TABLE session ADD COLUMN play_instance_id TEXT REFERENCES play_instance(id);
ALTER TABLE session ADD COLUMN launch_profile_id TEXT;
ALTER TABLE session ADD COLUMN instance_data_root TEXT;
PRAGMA user_version = 8;

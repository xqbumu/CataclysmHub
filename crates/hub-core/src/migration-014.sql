ALTER TABLE config_change ADD COLUMN rollback_of TEXT REFERENCES config_change(id);
PRAGMA user_version=14;

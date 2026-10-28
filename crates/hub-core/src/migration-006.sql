ALTER TABLE backup_archive ADD COLUMN display_name TEXT NOT NULL DEFAULT '';
ALTER TABLE backup_archive ADD COLUMN notes TEXT NOT NULL DEFAULT '';
ALTER TABLE backup_archive ADD COLUMN retention_locked INTEGER NOT NULL DEFAULT 0 CHECK(retention_locked IN (0,1));
ALTER TABLE backup_archive ADD COLUMN metadata_revision INTEGER NOT NULL DEFAULT 0 CHECK(metadata_revision >= 0 AND metadata_revision <= 9007199254740991);
ALTER TABLE backup_archive ADD COLUMN metadata_updated_at TEXT;
PRAGMA user_version = 6;

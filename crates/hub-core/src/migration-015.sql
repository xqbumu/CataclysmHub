CREATE TABLE hub_preferences (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  revision INTEGER NOT NULL,
  instance_profiles TEXT NOT NULL
);
INSERT INTO hub_preferences VALUES (1, 0, '{}');
PRAGMA user_version = 15;

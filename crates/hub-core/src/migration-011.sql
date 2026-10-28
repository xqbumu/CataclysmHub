CREATE TABLE content_entry (
  id TEXT PRIMARY KEY,
  root TEXT NOT NULL,
  kind TEXT NOT NULL CHECK(kind IN ('mod','soundpack')),
  snapshot TEXT NOT NULL,
  UNIQUE(root,kind)
);
PRAGMA user_version=11;

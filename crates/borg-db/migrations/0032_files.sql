CREATE TABLE IF NOT EXISTS files (
  file_id TEXT PRIMARY KEY,
  backend TEXT NOT NULL,
  storage_key TEXT NOT NULL,
  content_type TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  sha512 TEXT NOT NULL,
  owner_uri TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  deleted_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_files_sha512 ON files(sha512);
CREATE INDEX IF NOT EXISTS idx_files_deleted_at ON files(deleted_at);

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tracks (
  id           INTEGER PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  title        TEXT,
  artist       TEXT,
  album        TEXT,
  album_artist TEXT,
  track_no     INTEGER,
  disc_no      INTEGER,
  year         INTEGER,
  genre        TEXT,
  duration_ms  INTEGER,
  sample_rate  INTEGER,
  channels     INTEGER,
  bit_depth    INTEGER,
  mtime        INTEGER NOT NULL,  -- nanoseconds since the Unix epoch
  size         INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
CREATE INDEX IF NOT EXISTS idx_tracks_album  ON tracks(album);

-- The search index. It keeps its own copy of the text, rather than reading
-- `tracks`, so it can hold a column `tracks` has no copy of: the file name
-- without its directory or extension, which is all an untagged file has.
-- The name is worked out here, in SQL, so an older playr that adds rows to
-- `tracks` still keeps the index current. In the expression, `rtrim` with
-- every character but `/` strips a path back to its directory, and the same
-- with `.` strips a name back to its last dot; a name that is all extension,
-- such as `.hidden`, is kept whole. A Windows path, one that starts with a
-- drive, `C:\`, or with `\\`, has its backslashes read as `/` first; a
-- backslash elsewhere is part of a name, as Unix allows.
CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
  title, artist, album, album_artist, file, tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
  INSERT INTO tracks_fts(rowid, title, artist, album, album_artist, file)
  VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
          COALESCE(NULLIF(rtrim(rtrim(substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1), replace(substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1), '.', '')), '.'), ''),
                   substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1)));
END;

CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
  DELETE FROM tracks_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
  DELETE FROM tracks_fts WHERE rowid = old.id;
  INSERT INTO tracks_fts(rowid, title, artist, album, album_artist, file)
  VALUES (new.id, new.title, new.artist, new.album, new.album_artist,
          COALESCE(NULLIF(rtrim(rtrim(substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1), replace(substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1), '.', '')), '.'), ''),
                   substr((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), length(rtrim((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), replace((CASE WHEN substr(new.path, 2, 2) = ':\' OR substr(new.path, 1, 2) = '\\' THEN replace(new.path, '\', '/') ELSE new.path END), '/', ''))) + 1)));
END;

CREATE TABLE IF NOT EXISTS playlists (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS playlist_items (
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  PRIMARY KEY (playlist_id, position)
);

-- Positions marked in a track. Keyed by path rather than track id, so marks
-- survive a rescan that renumbers tracks and work for files outside the library.
CREATE TABLE IF NOT EXISTS marks (
  path  TEXT NOT NULL,
  frame INTEGER NOT NULL,  -- source frame index, exact at any playback speed
  rate  INTEGER NOT NULL,  -- the source sample rate `frame` counts in
  PRIMARY KEY (path, frame)
);

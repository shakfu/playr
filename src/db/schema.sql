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

CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
  title, artist, album, album_artist,
  content='tracks', content_rowid='id', tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
  INSERT INTO tracks_fts(rowid, title, artist, album, album_artist)
  VALUES (new.id, new.title, new.artist, new.album, new.album_artist);
END;

CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
  INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist);
END;

CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
  INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, album_artist)
  VALUES ('delete', old.id, old.title, old.artist, old.album, old.album_artist);
  INSERT INTO tracks_fts(rowid, title, artist, album, album_artist)
  VALUES (new.id, new.title, new.artist, new.album, new.album_artist);
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

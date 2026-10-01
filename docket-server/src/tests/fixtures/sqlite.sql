-- The schema of a SQLite docket database, the source an import reads.

CREATE TABLE IF NOT EXISTS projects (
  slug TEXT PRIMARY KEY,
  keys TEXT NOT NULL DEFAULT '[]',          -- JSON [{key, kind, meaning, turn}]
  remotes TEXT NOT NULL DEFAULT '[]',       -- JSON [origin url]
  themes TEXT NOT NULL DEFAULT '[]',        -- JSON [{name, note}]
  cite_roots TEXT NOT NULL DEFAULT '[]',    -- JSON [path], relative to a root binding
  repos TEXT NOT NULL DEFAULT '[]',         -- JSON [path], where a sha may resolve
  fleet_repo TEXT,
  integration_ref TEXT,
  worktree_hint TEXT,
  test_hint TEXT,
  skills TEXT NOT NULL DEFAULT '{}',      -- JSON {fact: text}, the per-project facts the skills render
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Local only, never dumped: where a project sits on this host.
CREATE TABLE IF NOT EXISTS roots (
  host TEXT NOT NULL,
  path TEXT NOT NULL,
  project TEXT NOT NULL REFERENCES projects(slug),
  bound_at TEXT NOT NULL,
  how TEXT NOT NULL,                        -- auto | bind | import
  PRIMARY KEY (host, path)
);

CREATE TABLE IF NOT EXISTS items (
  rid INTEGER PRIMARY KEY,
  project TEXT NOT NULL REFERENCES projects(slug),
  key TEXT NOT NULL,
  num INTEGER NOT NULL CHECK (num > 0),
  id TEXT GENERATED ALWAYS AS (key || num) STORED,
  title TEXT NOT NULL CHECK (title <> ''),
  state TEXT NOT NULL CHECK (state IN ('open', 'done', 'dropped')),
  turn TEXT CHECK (turn IN ('agent', 'user')),
  turn_note TEXT,
  asked_at TEXT,
  claim_branch TEXT,
  claim_host TEXT,
  claim_since TEXT,
  claim_runner TEXT,                        -- codex | claude | remote, when a fleet job holds the claim
  claim_job TEXT,                           -- that job's name in its runner
  claim_on TEXT,                            -- the host the job runs on, when it is not claim_host
  wait_on TEXT CHECK (wait_on IN ('item', 'condition')),
  wait_item INTEGER REFERENCES items(rid),
  wait_ref TEXT,
  wait_since TEXT,
  decision TEXT,
  decided_at TEXT,
  resolution TEXT,
  superseded_by INTEGER REFERENCES items(rid),
  scope TEXT CHECK (scope IN ('inbox', 'later')),  -- NULL: in the release
  complexity TEXT CHECK (complexity IN ('high', 'medium', 'low')),
  group_name TEXT,
  theme TEXT,
  rank INTEGER,
  tags TEXT NOT NULL DEFAULT '[]',
  body TEXT NOT NULL DEFAULT '',
  conflict INTEGER NOT NULL DEFAULT 0,
  opened_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (project, key, num),
  CHECK ((state = 'open') = (turn IS NOT NULL)),
  CHECK ((claim_branch IS NULL) = (claim_host IS NULL) AND (claim_branch IS NULL) = (claim_since IS NULL)),
  CHECK (claim_branch IS NULL OR (state = 'open' AND turn = 'agent')),
  CHECK ((wait_on IS NULL) = (wait_since IS NULL)),
  CHECK (wait_on IS NULL OR state = 'open'),
  CHECK (wait_on IS NOT 'item' OR wait_item IS NOT NULL),
  CHECK (wait_on IS NOT 'condition' OR wait_ref IS NOT NULL),
  CHECK (wait_on IS NOT NULL OR (wait_item IS NULL AND wait_ref IS NULL)),
  CHECK (wait_item IS NULL OR wait_item <> rid),
  CHECK (state = 'open' OR resolution IS NOT NULL),
  CHECK (superseded_by IS NULL OR state = 'dropped'),
  CHECK ((decision IS NULL) = (decided_at IS NULL))
);

CREATE INDEX IF NOT EXISTS items_project_state ON items(project, state);
CREATE INDEX IF NOT EXISTS items_wait ON items(wait_item);

CREATE TABLE IF NOT EXISTS events (
  seq INTEGER PRIMARY KEY,
  uid TEXT NOT NULL UNIQUE,
  project TEXT NOT NULL REFERENCES projects(slug),
  rid INTEGER REFERENCES items(rid),
  at TEXT NOT NULL,
  host TEXT NOT NULL,
  branch TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('opened', 'claimed', 'released', 'waited', 'resumed',
    'asked', 'replied', 'decided', 'closed', 'dropped', 'reopened', 'edited', 'renumbered',
    'claim_lost', 'queue', 'legacy', 'audited')),
  note TEXT,
  data TEXT                                 -- JSON {field: value}: a landing, a gates result, an audit
);

CREATE INDEX IF NOT EXISTS events_item ON events(rid, at);
CREATE INDEX IF NOT EXISTS events_project_at ON events(project, at);

CREATE TABLE IF NOT EXISTS links (
  rid INTEGER NOT NULL REFERENCES items(rid) ON DELETE CASCADE,
  kind TEXT NOT NULL CHECK (kind IN ('related', 'opened', 'cites_file', 'cites_test')),
  to_rid INTEGER REFERENCES items(rid) ON DELETE CASCADE,
  to_path TEXT,
  to_line INTEGER,
  CHECK ((to_rid IS NOT NULL) <> (to_path IS NOT NULL)),
  UNIQUE (rid, kind, to_rid, to_path, to_line)
);

CREATE INDEX IF NOT EXISTS links_to ON links(to_rid);
CREATE INDEX IF NOT EXISTS links_path ON links(to_path);

-- Rows changed by a committed transaction whose dump has not reached the git repo yet.
CREATE TABLE IF NOT EXISTS pending_dump (
  rid INTEGER PRIMARY KEY,
  project TEXT NOT NULL
);

-- Local only, never dumped: each chore's last run on this host, kept so a failure is seen.
CREATE TABLE IF NOT EXISTS chores (
  name TEXT NOT NULL,
  project TEXT NOT NULL DEFAULT '',
  at REAL,
  seconds REAL,
  result TEXT,
  failures INTEGER NOT NULL DEFAULT 0,
  ok INTEGER,
  PRIMARY KEY (name, project)
);

CREATE TABLE IF NOT EXISTS meta (
  k TEXT PRIMARY KEY,
  v TEXT NOT NULL
);

-- A plain FTS table holding its own copy of the text, so snippet() and highlight() work.
CREATE VIRTUAL TABLE IF NOT EXISTS items_fts USING fts5(
  id, title, body, files,
  tokenize='unicode61 tokenchars ''_-.'''
);

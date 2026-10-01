# docket

**A work register for a fleet of coding agents, served from one database.**

docket keeps every project's tickets, questions, plans and decisions in one SQLite database behind
an HTTP API. Agents claim, close and file items through it from any machine, and the server is the
one place a claim is decided, so two machines never take the same item.

## Crates

| Crate | What |
|---|---|
| `docket-core` | the rules: status words, claims, refusals, the queue order, the schema and the API types |
| `docket-server` | axum and sea-orm over the database: read routes, the write verbs under `/do`, key auth |
| `docket-dump` | the database written one way into a git checkout, and rebuilt from one |

## Quick Start

```bash
sqlite3 docket.db < docket-core/schema.sql
echo "laptop owner $(openssl rand -hex 32)" > keys
DOCKET_DB=docket.db DOCKET_KEYS=keys cargo run -p docket-server
```

The server listens on `127.0.0.1:7878` (set `DOCKET_LISTEN` to change it). Every route but
`/health` needs a key:

```bash
curl -H "Authorization: Bearer $KEY" http://127.0.0.1:7878/projects
```

## Keys

The keys file has one line per key, `host role key`, where role is `owner` or `agent`. The host is
what the server records on every event made with that key, so a client cannot claim to be another
machine. An agent key cannot open items with `new` or `add`.

## Routes

| Route | What |
|---|---|
| `GET /next`, `/todo`, `/questions`, `/research`, `/waiting`, `/derived`, `/wip`, `/groups`, `/done`, `/dropped` | the lists, with `project=SLUG` |
| `GET /status` | the flow counts and the summary |
| `GET /show/{id}`, `/search`, `/similar/{id}` | one item, full-text search over FTS5 |
| `GET /projects`, `/items`, `/events`, `/links` | the stored rows, filtered and paged |
| `GET /dump` | the rows changed after an event `seq` (`since=N`), or every row (`since=0`), for `docket-dump` |
| `POST /do/{verb}` | `new`, `add`, `start`, `close`, `release`, `drop`, `reopen`, `wait`, `resume`, `ask`, `reply`, `answer`, `decide`, `priority`, `rate`, `edit`, `link`, `fold`, `key`, `pull`, `defer` |

A refused write answers `409` with the reason, and nothing is written.

## Dump

`docket-dump` writes the database into a git checkout, one way: a file per item, and an event log
and a project file per project. It reads the server and key from `DOCKET_SERVER` and `DOCKET_KEY`,
or `~/.config/docket/client`.

```bash
docket-dump --repo ~/dump                          # the rows changed since the last pass, committed and pushed
docket-dump --repo ~/dump --every 300              # a pass every five minutes
docket-dump --repo ~/dump --full --no-push         # every row, committed only
docket-dump --repo ~/dump --restore --db new.db    # an empty database rebuilt from the checkout
```

The checkout keeps its cursor (the last event it holds) in its own git config as
`docket.dumpcursor`, never committed. A push that fails leaves its commits for the next pass.

## Docker

```bash
docker build -t docket-server .
docker run -v "$PWD/data:/data" -v "$PWD/keys:/etc/docket/keys:ro" -p 7878:7878 docket-server
```

The database is `/data/docket.db`, and the directory must be writable by uid 10001 (SQLite keeps
its `-wal` and `-shm` files beside it).

## Docker Compose

`compose.yaml` runs the server behind Traefik on `127.0.0.1:7878`. Point it at the database
directory and the keys file from a `.env` file beside it:

```bash
DOCKET_DATA=/path/to/data
DOCKET_KEYS_FILE=/path/to/keys
DOCKET_UID=1000
DOCKET_GID=1000
```

Then `docker compose up -d --build`. Set `DOCKET_READ_ONLY=1` to serve a database another program
writes: reads and the change stream follow its commits, and every write is refused with `405`.

## Caveats

- One server process owns the database. Run a single replica on local disk: SQLite's locks do not
  hold over NFS.
- The server creates no schema. Start it on a database made from `docket-core/schema.sql`.
- There is no route to create a project yet, so a fresh database serves only the projects already in
  it.

## License

MIT

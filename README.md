# docket

**A work register for a fleet of coding agents, served from one database.**

docket keeps every project's tickets, questions, plans and decisions in one Postgres database behind
an HTTP API. Agents claim, close and file items through it from any machine, and the server is the
one place a claim is decided, so two machines never take the same item.

## Crates

| Crate | What |
|---|---|
| `docket-core` | the rules: status words, claims, refusals, the queue order and the API types |
| `docket-migration` | the schema as migrations, applied by the server before it listens |
| `docket-server` | axum and sea-orm over the database: read routes, the write verbs under `/do`, key auth, the import from SQLite and the move onto plans |
| `docket` | the command line: every verb as a request to the server, printed as the text an agent reads |
| `docket-dump` | the database written one way into a git checkout, and rebuilt from one |

## Quick Start

```bash
docker run -d --name docket-pg -e POSTGRES_PASSWORD=docket -p 127.0.0.1:5432:5432 postgres:18
echo "laptop owner $(openssl rand -hex 32)" > keys
DATABASE_URL=postgres://postgres:docket@127.0.0.1:5432/postgres DOCKET_KEYS=keys cargo run -p docket-server
```

The server applies any migration the database lacks, then listens on `127.0.0.1:7878` (set
`DOCKET_LISTEN` to change it). Every route but `/health` needs a key:

```bash
curl -H "Authorization: Bearer $KEY" http://127.0.0.1:7878/projects
```

## Client

`docket` reads the server's address and its key from `~/.config/docket/client`, or from
`DOCKET_SERVER` and `DOCKET_KEY` over it:

```bash
printf 'server = http://127.0.0.1:7878\nkey = %s\n' "$KEY" > ~/.config/docket/client
cargo run -p docket -- status
```

A command works out its project from `-p SLUG`, then `DOCKET_PROJECT`, then the directories bound
on this machine (`~/.config/docket/roots`, written by `docket bind` and by the first command run in
a checkout), then the outermost git repository, matched to a project by its remote's slug, a shared
remote or its directory name, or created with the default keys.

## Keys

The keys file has one line per key, `host role key`, where role is `owner` or `agent`. The host is
what the server records on every event made with that key, so a client cannot claim to be another
machine. An agent key cannot open items with `new` or `add`.

## Routes

| Route | What |
|---|---|
| `GET /next`, `/todo`, `/questions`, `/research`, `/waiting`, `/derived`, `/wip`, `/groups`, `/done`, `/dropped` | the lists, with `project=SLUG` |
| `GET /status` | the flow counts and the summary |
| `GET /show/{id}`, `/search`, `/similar/{id}` | one item, full-text search over Postgres `tsvector` |
| `GET /context/{id}`, `/log/{id}`, `/deps/{id}` | what `show` prints beside an item, its events, its ties |
| `GET /flow`, `/summary`, `/check`, `/audit`, `/graph`, `/files`, `/citations`, `/shares` | a project's flow, status, integrity, audits, graph, cited files, claims sharing files |
| `GET /counts`, `/whoami` | every project's counts; the host and role of the key presented |
| `GET /projects`, `/items`, `/events`, `/links` | the stored rows, filtered and paged |
| `GET /dump` | the rows changed after an event `seq` (`since=N`), or every row (`since=0`), for `docket-dump` |
| `POST /do/{verb}` | `new`, `add`, `start`, `close`, `release`, `drop`, `reopen`, `wait`, `resume`, `ask`, `reply`, `answer`, `decide`, `priority`, `rate`, `edit`, `link`, `key`, `project`, `reindex` |

A refused write answers `409` with the reason, and nothing is written.

## Dump

`docket-dump` writes the database into a git checkout, one way: a file per item, and an event log
and a project file per project. It reads the server and key from `DOCKET_SERVER` and `DOCKET_KEY`,
or `~/.config/docket/client`.

```bash
docket-dump --repo ~/dump                          # the rows changed since the last pass, committed and pushed
docket-dump --repo ~/dump --every 300              # a pass every five minutes
docket-dump --repo ~/dump --full --no-push         # every row, committed only
docket-dump --repo ~/dump --restore --database-url postgres://...   # an empty database rebuilt from the checkout
```

The checkout keeps its cursor (the last event it holds) in its own git config as
`docket.dumpcursor`, never committed. A push that fails leaves its commits for the next pass.

## Import

`docket-server import --from FILE` copies a SQLite docket database into an empty Postgres one, after
applying the migrations. Every row keeps its rid, event `seq` and link id, so a dump checkout's cursor
still holds. The counts of every table are compared per project before the copy commits; a database
that already holds rows is refused. Copy the SQLite file with `.backup` first, never the file itself:

```bash
sqlite3 docket.db ".backup docket-copy.db"
DATABASE_URL=postgres://... docket-server import --from docket-copy.db
```

Data from before plans were the one grouping is moved onto that model with `docket-server simplify`.
Each package key holds plans from then on, ids unchanged, and every open plan waits on what it opened.
A review claim on a package is given back, and a plan that already held its audit round is closed.
The inbox, later and the release fold into priority: normal work there goes to low, and the release
fact is removed. Every change writes an event naming A7. Without `--write` it prints what each
project changes and writes nothing; a second run changes nothing.

```bash
DATABASE_URL=postgres://... docket-server simplify
DATABASE_URL=postgres://... docket-server simplify --write
```

## Docker

```bash
docker build -t docket-server .
docker run -e DATABASE_URL=postgres://... -v "$PWD/keys:/etc/docket/keys:ro" -p 7878:7878 docket-server
```

The keys file must be readable by uid 10001.

## Docker Compose

`compose.yaml` runs the server and Postgres behind Traefik at `http://docket.localhost`, on port 80 of
this machine only. Postgres keeps its data in the named volume `postgres` and is not published.
Set the database password and the keys file in a `.env` file beside it:

```bash
POSTGRES_PASSWORD=$(openssl rand -hex 24)
DOCKET_KEYS_FILE=/path/to/keys
DOCKET_UID=1000
DOCKET_GID=1000
```

Then `docker compose up -d --build`. The server waits for Postgres to be healthy and migrates it on
its first start.

To move a SQLite docket database in, import a copy before the server first takes writes. The copy is
mounted read-only and must be readable by `DOCKET_UID`:

```bash
sqlite3 /path/to/docket.db ".backup $PWD/docket-copy.db"
docker compose build docket-server
docker compose up -d postgres
docker compose run --rm -v "$PWD/docket-copy.db:/import/docket.db:ro" docket-server import --from /import/docket.db
docker compose run --rm docket-server simplify --write
docker compose up -d
```

The import prints the rows of every table by project and ends with `every count matches`. Run against
a database that already holds rows, it changes nothing and says so.

## Tests

The tests need a Postgres on which they can create databases, named by `DATABASE_URL`. Each test makes
a database of its own and drops it when done:

```bash
docker run -d --name docket-test-pg -e POSTGRES_PASSWORD=pg -p 127.0.0.1:5433:5432 postgres:18
DATABASE_URL=postgres://postgres:pg@127.0.0.1:5433/postgres cargo test
```

## Caveats

- Writes take one advisory lock for their transaction, so they commit one at a time and their events
  become visible in `seq` order, which the dump's cursor relies on. Reads run side by side.
- Search ranks with Postgres `ts_rank` over title, id, cited paths and body, weighted in that order,
  and cuts its snippets with `ts_headline`.

## License

MIT

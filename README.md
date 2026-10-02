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
| `docket-server` | axum and sea-orm over the database: read routes, the write verbs under `/do`, key auth, the import from SQLite |
| `docket` | the command line: every verb as a request to the server, printed as the text an agent reads |
| `docket-dump` | the database written one way into a git checkout, and rebuilt from one |

`docket-web` is the web client, a Svelte page the server serves.

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

## Web

`docket-web` does in the browser what the screen does: every project, a project's flow with what is running and what
waits on you, the lists beside an item in full, the plans with their progress, the log and the facts. Every verb is a
button on the item, and the page reads again whenever the database moves. `Ctrl K` goes to an item, a page or a
project, and `?` lists the keys.

The server serves it at `/ui/` from the directory `DOCKET_WEB` names, and sends `/` there. The Docker image builds it
and sets `DOCKET_WEB`. The page asks for a key once and keeps it in the browser:

```bash
cd docket-web && npm ci && npm run build
DOCKET_WEB=$PWD/dist DATABASE_URL=postgres://... DOCKET_KEYS=keys cargo run -p docket-server
```

`npm run dev` serves it at `http://localhost:5190/ui/` and passes `/api` on to the server `DOCKET_SERVER` names
(`http://docket.localhost` by default), so writes made there land in that server's database.

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
| `GET /ui/` | the web client when `DOCKET_WEB` is set, with `/` sent to it; neither asks for a key |
| `POST /do/{verb}` | `new`, `add`, `start`, `close`, `release`, `drop`, `reopen`, `wait`, `resume`, `ask`, `reply`, `answer`, `decide`, `priority`, `rate`, `edit`, `link`, `fold`, `key`, `pull`, `defer`, `project`, `reindex` |

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

## Docker

```bash
docker build -t docket-server .
docker run -e DATABASE_URL=postgres://... -v "$PWD/keys:/etc/docket/keys:ro" -p 7878:7878 docket-server
```

The keys file must be readable by uid 10001.

## Docker Compose

`compose.yaml` runs the server and Postgres behind Traefik at `http://docket.localhost`, on port 80 of
this machine only, with the web client at `http://docket.localhost/ui/`. Postgres keeps its data in the
named volume `postgres` and is not published.
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

The web client's tests need no server:

```bash
cd docket-web && npm test && npm run check
```

## Caveats

- The web client keeps its key in the browser's local storage until you sign out, so sign out on a machine you share.
- Writes take one advisory lock for their transaction, so they commit one at a time and their events
  become visible in `seq` order, which the dump's cursor relies on. Reads run side by side.
- Search ranks with Postgres `ts_rank` over title, id, cited paths and body, weighted in that order,
  and cuts its snippets with `ts_headline`.

## License

MIT

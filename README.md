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

## Skills

Work is done by sessions, each taking one role with its skill, for Claude Code or Codex, as many at
once as wanted:

| Skill | What |
|---|---|
| `/plan` | a goal into a plan and its tickets; investigations and decided questions into tickets |
| `/work` | the next ticket: a worktree off the branch the session started from, the failing test first, merged back and closed |
| `/audit` | a plan whose tickets are all closed, checked once against what it asked; gaps filed as tickets |
| `/lead` | the project's one lead: every ticket, plan and due audit dispatched as a job on a machine with a free slot, on the model its complexity gives, then merged and closed |

The templates are in `skills/` and the client carries them. A skill names no project: each reads
the project's facts with `docket skills`. `docket skills install` writes them to `~/.claude/skills`
and `~/.agents/skills` and removes the skills they replace, after listing each change and asking
(`--yes` with no terminal; `docket skills diff` only lists). `docket instructions install` puts the
short docket block, `skills/docket-block.md`, into the AGENTS.md at the project's root the same way.

## Jobs

A lead claims an item and starts a job for it on a machine, locally or over ssh, once the job's
branch is in that machine's checkout of the project:

```bash
docket -p OWNER/REPO --branch lead/t14-123 job run --id T14 --runner claude --model MODEL --effort high --role build
docket job status            # every job here: running, done, failed or lost, and its report
docket job log lead-t14-123  # the last lines of its event stream
docket job kill lead-t14-123 # stops its whole process group
```

`run` adds a worktree beside the checkout on that branch, writes the role's brief from `skills/job/`
(build, audit or plan) and starts `claude -p` or `codex exec` headless in a process group of its own,
then returns. The job's directory under `$XDG_STATE_HOME/docket/jobs` (or `DOCKET_JOB_STATE`) holds
`meta.json`, the brief, the event stream, the pid and the exit code. A job builds and commits on its
branch and ends its final message with `DONE <sha>`, `WAITING Q<n>` or `FAILED <reason>`; `docket`
refuses `start`, `release`, `close`, `drop` and `reopen` inside one (`DOCKET_JOB` set), which are
the lead's. `DOCKET_JOB_RUNNER_CLAUDE` and `DOCKET_JOB_RUNNER_CODEX` replace the program run.

## Dispatch

A lead moves work with three commands, run in its checkout on the branch the work merges into:

```bash
docket dispatch T14 [--on NAME] [--runner R --model M --effort E] [--role build|audit|plan]
docket jobs [--wait] [--timeout 300] [--all]   # every machine's jobs, read where they run
docket collect T14 [--discard]                 # the job's change committed here, the job cleared
```

`dispatch` claims the item on a fresh `lead/` branch, recording the runner, job, model, machine and
role on the claim, pushes this checkout's head to the machine's checkout as that branch, and starts
the job there with `docket job run`. The model comes from the project's `models` fact by the item's
complexity unless given; the machine is the one with the runner and the most free slots, this one
first among equals, unless `--on` names it. A failed push or start gives the claim back. Another
machine is reached over ssh at its recorded address (`DOCKET_SSH` replaces the program), running
the `docket` on its path. Code moves only by git from the lead's machine, and commits are made
there only: the base is pushed to the job's machine as its branch, the job leaves its change
uncommitted and ends with a `MESSAGE` line, and `collect` takes the change (`docket job diff` on
that machine), commits it on the lead's machine with that message, and clears the job. A change
inside a submodule becomes a commit in the lead's own clone of that submodule, on a branch named
for the job and pushed nowhere, and the job's commit points at it. A change that cannot be
committed leaves the job where it ran, to collect again. `jobs --wait` returns once one of the running jobs ends.

## Private names

A repository may be public while its docket is private. `docket private check` reads the private
names from the server on the owner's key (`GET /private`: project slugs, owners, machines and their
addresses, the hosts on keys, events and claims, and every item title long enough to be its own),
adds this machine's user name, home and ssh host aliases, and searches the tracked files, the staged
change (`--staged`), a commit message (`--message FILE`) or every commit of a range (`--range
origin/main..HEAD`, `--range --all`). It also finds an address on a private network and an access
token by their shape. A name in lower case is matched in any case, one with a capital as written. A
hit prints `file:line: name` and exits 1. `--ids` also lists the docket item ids that code comments
cite, for a person to read.

The names are never written into the repository. A name that is public (a product's name, a public
dependency) is listed one a line in `$XDG_CONFIG_HOME/docket/public`, outside it; the owner and name
of `origin` are public already. A private name the docket cannot know (another machine's alias for
this one) is listed the same way in `$XDG_CONFIG_HOME/docket/private`. A licence, which names its holder by design, is not searched.
`docket private hook` installs a pre-commit hook (the staged change), a commit-msg hook and a
pre-push hook (every commit the push adds, then the tree). `docket private terms` prints the names,
for a history rewrite's replace file kept outside the repository.

## Keys

The keys file has one line per key, `host role key`, where role is `owner` or `agent`. The host is
what the server records on every event made with that key, so a client cannot claim to be another
machine. An agent key cannot open items with `new` or `add`.

## Machines and the lead

The machines jobs run on are rows on the server, so every machine reads the same list and none of
them is named in a repository or a skill. Each has a name, which is the host on its key, the ssh
address the other machines reach it at (`user@host`, or `ssh://user@host:port` for another port), its job slots (1 to 64) and the runners it has (`claude`,
`codex`). The owner's key sets them:

```bash
docket machine set NAME --ssh ADDRESS --slots 4 --runners claude,codex [--note "..."]
docket machine set NAME --slots 8      # a set changes only the fields it names
docket machine remove NAME
docket machines                        # every machine, * against the one this client runs on
```

A project has at most one lead: a session that takes the project's lead claim and renews it while it
runs. `docket lead take` takes it, `renew` keeps it, `give` hands it back and `show` (the default)
says who holds it. The holder is the key's host and a session name, the branch acting unless
`--session` names one. A take while another holder renewed within the project's `lead_lapse`
(minutes, 10 by default) is refused and names the holder; once the claim lapses, the next take takes
it over. A take, a takeover and a give each write a `lead` event on the project; a renewal does not.

## Routes

| Route | What |
|---|---|
| `GET /next`, `/todo`, `/questions`, `/research`, `/waiting`, `/derived`, `/wip`, `/groups`, `/done`, `/dropped` | the lists, with `project=SLUG` |
| `GET /status` | the flow counts and the summary |
| `GET /show/{id}`, `/search`, `/similar/{id}` | one item, full-text search over Postgres `tsvector` |
| `GET /context/{id}`, `/log/{id}`, `/deps/{id}` | what `show` prints beside an item, its events, its ties |
| `GET /summary`, `/check`, `/audit`, `/graph`, `/files`, `/citations`, `/shares` | a project's status, integrity, audits, graph, cited files, claims sharing files |
| `GET /private` | the names private to the owner's docket, for `docket private check`; owner key only |
| `GET /counts`, `/whoami` | every project's counts; the host and role of the key presented |
| `GET /projects`, `/items`, `/events`, `/links` | the stored rows, filtered and paged |
| `GET /dump` | the rows changed after an event `seq` (`since=N`), or every row (`since=0`), for `docket-dump` |
| `GET /machines`, `POST /do/machine` | every machine; one set or removed (`remove: true`), on the owner's key |
| `GET /lead`, `POST /do/lead` | a project's lead claim; `act` take, renew or give, with the holder's `session` |
| `GET /ui/` | the web client when `DOCKET_WEB` is set, with `/` sent to it; neither asks for a key |
| `POST /do/{verb}` | `new`, `add`, `start`, `close`, `release`, `drop`, `reopen`, `wait`, `resume`, `ask`, `reply`, `answer`, `decide`, `priority`, `rate`, `edit`, `link`, `parent`, `key`, `project`, `reindex` |

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
The inbox and later fold into priority: normal work there goes to low. The release fact becomes the
`releases` fact: the release, then every theme named as a version (`1.1`, `v2.0`) in version
order, and release work keeps its priority. `docket next` orders by release, then priority, then
age: an item's theme names its release, and no theme or a theme the list leaves out is the current
release's. Every change writes an event saying why. Without `--write` it prints what each
project changes and writes nothing; a second run changes nothing.

```bash
DATABASE_URL=postgres://... docket-server simplify
DATABASE_URL=postgres://... docket-server simplify --write
```

The move onto the core (releases as rows, dependencies, one parent plan, an origin link, labels, a
priority column and assignment rows) is planned by one function, `docket_core::migrate::plan`, over
the rows the full dump carries. `docket admin migrate --dry-run` prints every change it makes to the
bound project (`--all` for every project), then each risky case and what is done about it: a plan
held by work in a later release, the edge that closes a cycle (kept as `related`), an item under two
plans (the earliest open one is its parent), a dependency in a later release (pulled into its
dependant's), a theme that is not a release, and the change to the dump's item files. A case whose
repair turns on an undecided rule waits on it, and nothing is written while one does. `--held` and
`--areas` decide those rules for one run, so the dry run shows each choice.

```bash
docket admin migrate --dry-run
docket admin migrate --dry-run --all --held detach --areas current
```

The parent slice is applied when the server migrates: each item's `opened` links split as the plan
splits them, into one parent plan (`items.parent_rid`, set with `docket parent T1 A3`) and `origin`
links (`docket link T1 origin Q2`), which record what spawned an item and add no structure. A dump
checkout or SQLite store written before then splits the same way on restore or import.

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

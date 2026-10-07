//! The command line: every verb, its arguments and the flags taken before or after it.

use clap::{Args, CommandFactory, Parser, Subcommand};

const COMPLEXITIES: [&str; 3] = ["high", "medium", "low"];
const PRIORITIES: [&str; 4] = ["critical", "high", "normal", "low"];
const TURNS: [&str; 2] = docket_core::assignment::TURNS;
const STATES: [&str; 4] = ["open", "done", "dropped", "any"];
const RUNNERS: [&str; 3] = docket_core::assignment::CLAIM_RUNNERS;
const ROLES: [&str; 5] = docket_core::assignment::ROLES;
const OUTCOMES: [&str; 6] = docket_core::assignment::OUTCOMES;
const NEEDS: [&str; 4] = docket_core::queue::NEEDS;
const WAITS: [&str; 2] = ["item", "condition"];
const QUEUE_ROLES: [&str; 3] = ["plan", "work", "audit"];
const JOB_ROLES: [&str; 3] = ["build", "audit", "plan"];

#[derive(Parser, Debug)]
#[command(
    name = "docket",
    about = "The fleet work register, as a client of the docket server.",
    infer_long_args = true
)]
pub struct Cli {
    /// project slug; default: the one bound to this directory
    #[arg(short = 'p', long, global = true)]
    pub project: Option<String>,
    /// machine output
    #[arg(long, global = true)]
    pub json: bool,
    /// the branch acting; default: git HEAD here
    #[arg(long, global = true)]
    pub branch: Option<String>,
    /// bare command: print and clear instead of the screen
    #[arg(long)]
    pub plain: bool,
    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Args, Debug, Default, Clone)]
pub struct Queue {
    pub n: Option<i64>,
    #[arg(long, value_parser = COMPLEXITIES)]
    pub complexity: Option<String>,
    #[arg(long)]
    pub key: Option<String>,
    /// only items carrying this label, given or from a plan above them
    #[arg(long, alias = "theme")]
    pub label: Option<String>,
    /// only items in this area, by name
    #[arg(long)]
    pub area: Option<String>,
    /// only items changing this repository, a path under the project root: their repo label, else
    /// the checkout fact
    #[arg(long, value_name = "PATH")]
    pub repo: Option<String>,
    /// only items of this release, a name or current
    #[arg(long)]
    pub release: Option<String>,
    /// only items a plan opened, at any depth
    #[arg(long, value_name = "ID")]
    pub under: Option<String>,
    /// only what these roles take, comma-separated and ranked in the order given within a
    /// release: plan (new plans, investigations, decided questions), work (tickets), audit
    /// (plans whose tickets are all closed)
    #[arg(long, value_parser = QUEUE_ROLES, value_delimiter = ',')]
    pub role: Vec<String>,
    /// only this priority and anything more urgent
    #[arg(long, value_parser = PRIORITIES)]
    pub priority: Option<String>,
    /// only the current release, the first not shipped; later releases and the backlog are left out
    #[arg(long)]
    pub current_release: bool,
}

#[derive(Args, Debug, Clone)]
pub struct OwnerQueue {
    pub n: Option<i64>,
    /// only items carrying this label, given or from a plan above them
    #[arg(long, alias = "theme")]
    pub label: Option<String>,
    /// only items in this area, by name
    #[arg(long)]
    pub area: Option<String>,
    /// only this key
    #[arg(long)]
    pub key: Option<String>,
    /// only this priority and anything more urgent
    #[arg(long, value_parser = PRIORITIES)]
    pub priority: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct Recent {
    pub n: Option<i64>,
    #[arg(long)]
    pub key: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// counts, claims, checks and the next few as text (the bare command on a pipe)
    Status,
    /// the agent queue: open, agent's turn, unclaimed, not waiting, most urgent then oldest first
    Next(Queue),
    /// shorthand for next --complexity high
    Complex {
        n: Option<i64>,
        /// only items carrying this label
        #[arg(long, alias = "theme")]
        label: Option<String>,
    },
    /// the owner's queue: derived answers to confirm, questions, then asks by what the owner must hold or do
    Todo(OwnerQueue),
    /// claimed items, by host
    Wip {
        #[arg(long)]
        host: Option<String>,
    },
    /// items nobody can move yet, by what they wait on
    #[command(alias = "blocked")]
    Waiting {
        #[arg(long, value_parser = WAITS)]
        on: Option<String>,
    },
    /// open decisions, by area
    #[command(alias = "q")]
    Questions {
        /// only questions carrying this label
        #[arg(long, alias = "theme")]
        label: Option<String>,
    },
    /// decided questions that owe work items
    Research,
    /// decisions agents derived, with their basis: the owner's digest
    Derived { n: Option<i64> },
    /// recently done, newest first
    Done(Recent),
    /// recently dropped, newest first
    Dropped(Recent),
    /// what a release closed, grouped by area
    Changelog {
        /// the release; the current one by default
        release: Option<String>,
        /// head each group of tickets with its plan's title
        #[arg(long)]
        plans: bool,
    },
    /// items meant to be taken together
    Groups { name: Option<String> },
    /// one item in full
    Show { id: String },
    /// one item's events, the conversation on it
    Log { id: String },
    /// every project, its counts and its roots here
    Projects,
    /// open an item; the id is allocated inside the transaction
    New {
        key: String,
        title: String,
        /// FILE or - for stdin
        #[arg(long)]
        body: Option<String>,
        #[arg(long, value_parser = TURNS)]
        turn: Option<String>,
        #[arg(long, value_parser = COMPLEXITIES)]
        complexity: Option<String>,
        #[arg(long, value_parser = PRIORITIES)]
        priority: Option<String>,
        /// a label to give it, by name
        #[arg(long)]
        theme: Option<String>,
        /// the release it is filed for: current or a release not shipped; none files it in the backlog; required in a job
        #[arg(long, value_name = "RELEASE")]
        release: Option<String>,
        /// the group it is taken with: the label group:NAME
        #[arg(long)]
        group: Option<String>,
        /// a repository it changes, a path under the project root or @OWNER/NAME for another
        /// project's: the label repo:PATH; give it once for each repository; under a plan with
        /// one, the plan's
        #[arg(long, value_name = "PATH")]
        repo: Vec<String>,
        /// the area it is filed in; under a plan it is the plan's, and naming another is refused
        #[arg(long, value_name = "AREA")]
        area: Option<String>,
        /// the plan it is filed under; without --release it takes the plan's release, and it may not name a later one
        #[arg(long, value_name = "PLAN")]
        parent: Option<String>,
    },
    /// file a side finding as a low-priority ticket
    Add {
        title: String,
        /// the work key to file under; default B, or the first work key
        #[arg(long)]
        key: Option<String>,
        /// FILE or - for stdin
        #[arg(long)]
        body: Option<String>,
        /// the fleet job that saw it, so the brakes count it
        #[arg(long = "from", value_name = "JOB")]
        from: Option<String>,
        /// the release it is filed for: current or a release not shipped; none files it in the backlog; required in a job
        #[arg(long, value_name = "RELEASE")]
        release: Option<String>,
        /// the area it is filed in; under a plan it is the plan's, and naming another is refused
        #[arg(long, value_name = "AREA")]
        area: Option<String>,
        /// the plan it is filed under, which gives it the plan's area
        #[arg(long, value_name = "PLAN")]
        parent: Option<String>,
    },
    /// claim an item on this branch
    Start {
        id: String,
        #[arg(long)]
        force: bool,
        /// the fleet runner whose job holds the claim
        #[arg(long, value_parser = RUNNERS)]
        runner: Option<String>,
        /// that job's name in its runner, recorded on the claim
        #[arg(long)]
        job: Option<String>,
        /// the model the runner uses, recorded on the claim
        #[arg(long)]
        model: Option<String>,
        /// the host the job runs on, when it is not this one
        #[arg(long, value_name = "HOST")]
        on: Option<String>,
        /// the job's role, recorded on the claim
        #[arg(long, value_parser = ROLES)]
        role: Option<String>,
    },
    /// give a claim back
    #[command(alias = "release")]
    Unclaim {
        id: String,
        note: Option<String>,
        /// the loop sending it back: counted, and twice parks it
        #[arg(long)]
        bounce: bool,
        /// its finished branch did not land: the next job rebases it
        #[arg(long, value_name = "BRANCH")]
        rebase: Option<String>,
        /// the agent runner that did the work
        #[arg(long)]
        runner: Option<String>,
        /// the model that did the work
        #[arg(long)]
        model: Option<String>,
        /// how the attempt ended, recorded on its assignment
        #[arg(long, value_parser = OUTCOMES, required = true)]
        outcome: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// done, under a sha or what closed it; unblocks waiters
    #[command(alias = "built")]
    Close {
        id: String,
        resolution: Option<String>,
        /// the gates result the landing ran: passed, failed or skipped: why
        #[arg(long)]
        gates: Option<String>,
        /// the agent runner that did the work
        #[arg(long)]
        runner: Option<String>,
        /// the model that did the work
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// closed without doing: superseded, archived, will not fix
    Drop {
        id: String,
        why: Option<String>,
        #[arg(long)]
        superseded_by: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// a done or dropped item back to open, with a reason
    Reopen {
        id: String,
        why: String,
        /// the area it reopens in, needed when its own holds closed items only
        #[arg(long)]
        area: Option<String>,
        /// the release it reopens in: current, a release not shipped, or "" for the backlog; needed when its own has shipped
        #[arg(long)]
        release: Option<String>,
        /// with --release: move too what the reopen would leave out of order
        #[arg(long)]
        carry: bool,
        /// reopen although it would leave items out of release order
        #[arg(long)]
        force: bool,
    },
    /// park an item behind another, or until a condition, which becomes a task for the owner
    #[command(alias = "block")]
    Wait {
        id: String,
        /// an item id
        #[arg(long)]
        on: Option<String>,
        /// a condition, in words
        #[arg(long)]
        until: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// clear a wait by hand, and every dependency still holding the item
    Resume { id: String, note: Option<String> },
    /// what an item depends on: it waits until every dependency is satisfied
    Dep {
        #[command(subcommand)]
        what: DepCmd,
    },
    /// hand an item to the owner: their turn, with what is needed
    #[command(aliases = ["manual", "park"])]
    Ask {
        id: String,
        note: String,
        /// what the owner must do: hold a device or thing, have an account or store, act from their
        /// machine, or judge
        #[arg(long, value_parser = NEEDS)]
        need: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// hand it back to the agents, with what happened
    Reply { id: String, note: String },
    /// record a decision on a question; unblocks what waited on it
    Answer {
        id: String,
        decision: String,
        /// the decision, central idea or practice an agent derived it from
        #[arg(long, value_name = "BASIS")]
        derived: Option<String>,
        /// the items already filed that carry the decision out: the question closes on them
        #[arg(long, value_name = "ID", value_delimiter = ',')]
        carried_by: Vec<String>,
    },
    /// record a choice made on an item by best practice, with its basis
    Decide {
        id: String,
        #[arg(required_unless_present = "area")]
        choice: Option<String>,
        /// the decision, central idea or practice it rests on
        #[arg(long, required = true)]
        basis: String,
        /// place the item in this area, before areas exist; the choice defaults to "area NAME"
        #[arg(long, value_name = "NAME")]
        area: Option<String>,
        /// what the area is about, with --area
        #[arg(long, value_name = "TEXT", requires = "area")]
        about: Option<String>,
    },
    /// set complexity
    Rate {
        id: String,
        #[arg(value_parser = COMPLEXITIES)]
        level: String,
    },
    /// set how soon items are worked: critical, high, normal or low
    Priority {
        #[arg(required = true, value_name = "ID")]
        ids: Vec<String>,
        #[arg(value_parser = PRIORITIES)]
        tier: String,
    },
    /// fields, an appended note, or the whole body
    Edit {
        id: String,
        /// title, complexity, theme (gives that label), group (its group:NAME label), tags (its own labels) or `turn_note`; priority has `docket priority`
        #[arg(long = "set", value_name = "FIELD=VALUE")]
        set: Vec<String>,
        /// the release it moves to: current, a release not shipped, or "" for the backlog
        #[arg(long, value_name = "RELEASE")]
        release: Option<String>,
        /// with --release: move too what the move would leave out of order, dependants and plans later, dependencies and children earlier
        #[arg(long)]
        carry: bool,
        /// the area it and every item under it move to; refused on an item under a plan, whose area is the plan's
        #[arg(long, value_name = "AREA")]
        area: Option<String>,
        /// the repositories it changes, replacing those it has: once for each, a path under the
        /// project root or @OWNER/NAME; "" takes them away
        #[arg(long, value_name = "PATH")]
        repo: Vec<String>,
        #[arg(long, value_name = "TEXT")]
        append: Option<String>,
        #[arg(long, value_name = "FILE|-")]
        body: Option<String>,
    },
    /// a project's releases: list them, add one, move one's open items, or ship one
    Releases {
        /// list (the default), add, move or ship
        #[arg(value_parser = ["list", "add", "move", "ship"])]
        action: Option<String>,
        /// the release: add takes a version, placed by its version order
        name: Option<String>,
        /// move: where its open items go, current, a release, or "" for the backlog
        #[arg(long, value_name = "RELEASE")]
        to: Option<String>,
        /// ship: move what is still open there first; without it ship refuses while any is open
        #[arg(long = "move-open-to", value_name = "RELEASE")]
        move_open_to: Option<String>,
        /// move and ship: move too what the move would leave out of order, dependants and plans later, dependencies and children earlier
        #[arg(long)]
        carry: bool,
        /// add: the date it is meant to ship
        #[arg(long, value_name = "DATE")]
        target: Option<String>,
        /// add: a line on what it is for
        #[arg(long)]
        note: Option<String>,
        /// list: the shipped releases too
        #[arg(long)]
        all: bool,
    },
    /// a project's areas: list them, add one, edit one, move one to another place, or remove one
    Areas {
        /// list (the default), add, edit, move or rm
        #[arg(value_parser = ["list", "add", "edit", "move", "rm"])]
        action: Option<String>,
        /// the area
        name: Option<String>,
        /// add and edit: what the area holds
        #[arg(long, value_name = "TEXT")]
        about: Option<String>,
        /// edit: its new name
        #[arg(long = "name", value_name = "NAME")]
        rename: Option<String>,
        /// add and edit: critical, high, normal or low; "" for none
        #[arg(long, value_name = "WORD")]
        priority: Option<String>,
        /// move: the place it takes, the first being 1
        #[arg(long, value_name = "PLACE")]
        to: Option<i64>,
    },
    /// give an item a label, or take one from it; a plan's labels are read by every item under it
    Label {
        /// add or rm
        #[arg(value_parser = ["add", "rm"])]
        action: String,
        id: String,
        name: String,
        /// add: what the label means; replaces the description it has
        #[arg(long, value_name = "TEXT")]
        about: Option<String>,
    },
    /// a project's labels with their descriptions and how many items were given each
    Labels,
    /// A related B, or B the origin of A, what spawned it; several A at once
    Link {
        /// A... related|origin B
        #[arg(required = true, num_args = 3.., value_name = "A KIND B")]
        words: Vec<String>,
        #[arg(long)]
        remove: bool,
    },
    /// A under plan B, its one parent; several A at once
    Parent {
        /// A... PLAN, or A... alone with --none
        #[arg(required = true, num_args = 1.., value_name = "A PLAN")]
        words: Vec<String>,
        /// take A out of its plan
        #[arg(long)]
        none: bool,
    },
    /// where a plan, story, package, idea, group, label or area stands
    Audit {
        id: Option<String>,
        #[arg(long)]
        group: Option<String>,
        /// every item carrying a label, given or from a plan above it
        #[arg(long, alias = "theme")]
        label: Option<String>,
        /// an area whole: its description, its priority and every item carrying it
        #[arg(long)]
        area: Option<String>,
    },
    /// items, links and cited files, as JSON or Graphviz dot
    Graph {
        #[arg(long)]
        dot: bool,
        /// leave out the cited files
        #[arg(long)]
        no_files: bool,
    },
    /// full-text search, ranked, with a snippet
    Search {
        #[arg(required = true)]
        words: Vec<String>,
        #[arg(long)]
        key: Option<String>,
        #[arg(long, value_parser = STATES, default_value = "any")]
        state: String,
        #[arg(short = 'n', default_value_t = 20)]
        n: i64,
        /// pass the FTS5 query through untouched
        #[arg(long)]
        raw: bool,
        /// only items carrying this label
        #[arg(long, alias = "theme")]
        label: Option<String>,
        /// only items not carrying this label
        #[arg(long, alias = "without-theme")]
        without_label: Option<String>,
    },
    /// items close to this one by title, files and symbols
    Similar {
        id: String,
        n: Option<i64>,
        #[arg(long, value_parser = STATES, default_value = "any")]
        state: String,
    },
    /// everything linked to an item, both directions
    Deps { id: String },
    /// items citing a path
    Files {
        prefix: String,
        #[arg(long, value_parser = STATES, default_value = "any")]
        state: String,
    },
    /// integrity: conflicts, cycles, stale waits
    Check {
        /// diff every row against its dump file
        #[arg(long)]
        deep: bool,
    },
    /// rebuild the search rows and citation links from every item's body
    #[command(hide = true)]
    Reindex,
    /// build the published ref from the work ref's landings: print each proposed commit, or with
    /// --messages write them; never touches the work ref and never pushes
    Squash {
        /// one commit message per proposed commit, a line each, oldest first
        #[arg(long, value_name = "FILE")]
        messages: Option<String>,
        /// merge adjacent proposed commits until at most this many remain
        #[arg(long, value_name = "N")]
        cap: Option<usize>,
    },
    /// citations that no longer resolve
    Stale {
        #[arg(long)]
        open_only: bool,
        /// delete the job branches nothing needs that hold no commits the work ref lacks
        #[arg(long)]
        prune: bool,
    },
    /// the facts a project's skills read: show, KEY, KEY "value", get KEY, set KEY "value"
    Skills {
        /// show (the default), get, set, install or diff; or a fact name to show it, with a value to set it
        #[arg(value_parser = clap::builder::PossibleValuesParser::new(crate::cmd::skills::words()))]
        what: Option<String>,
        /// set and get: the fact
        key: Option<String>,
        /// set: the text; omit it to unset the fact
        value: Option<String>,
        /// install and diff: only ~/.claude/skills
        #[arg(long)]
        claude: bool,
        /// install and diff: only ~/.agents/skills
        #[arg(long)]
        codex: bool,
        /// install without asking, for a run with no terminal
        #[arg(short = 'y', long)]
        yes: bool,
        /// set: the owner-level value of an agent setting, read by every project that does not set
        /// its own
        #[arg(long)]
        all_projects: bool,
    },
    /// the docket block in the AGENTS.md at the project's root: install writes it, diff lists what
    /// install would change
    #[command(hide = true)]
    Instructions {
        #[arg(value_parser = ["install", "diff"], default_value = "diff")]
        what: String,
        /// install without asking, for a run with no terminal
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// a job under a lead on this machine: run one, list them, stop one, read its events
    #[command(hide = true)]
    Job {
        #[command(subcommand)]
        what: JobCmd,
    },
    /// the machines jobs run on, this one marked
    Machines,
    /// set or remove a machine, on the owner's key; set changes only the fields given
    #[command(hide = true)]
    Machine {
        #[arg(value_parser = ["set", "remove"])]
        what: String,
        /// the host its key names
        name: String,
        /// the ssh address the other machines reach it at: user@host, or ssh://user@host:port
        #[arg(long)]
        ssh: Option<String>,
        /// the most jobs it runs at once, 1 to 64
        #[arg(long)]
        slots: Option<i64>,
        /// the runners it has, as claude,codex
        #[arg(long)]
        runners: Option<String>,
        /// a line about it; empty clears it
        #[arg(long)]
        note: Option<String>,
        /// the directories put before PATH in the command run on it over ssh, as the shell reads
        /// them; empty clears it, none means $HOME/.local/bin:$HOME/.cargo/bin
        #[arg(long)]
        path: Option<String>,
    },
    /// the project's lead claim: show who leads, or take, renew or give it back
    #[command(hide = true)]
    Lead {
        #[arg(value_parser = ["show", "take", "renew", "give"], default_value = "show")]
        what: String,
        /// the session holding the claim on this host; default: the branch acting
        #[arg(long)]
        session: Option<String>,
    },
    /// a lead's dispatch: claim the item on a fresh branch, push the base to a machine with a free
    /// slot and start a job there, on the model the models fact gives its complexity
    #[command(hide = true)]
    Dispatch {
        id: String,
        /// the machine to run it on; default: the one with the most free slots for the runner
        #[arg(long)]
        on: Option<String>,
        #[arg(long, value_parser = crate::job::RUNNERS)]
        runner: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        effort: Option<String>,
        /// default: audit for a plan, plan for an investigation, build for the rest
        #[arg(long, value_parser = JOB_ROLES)]
        role: Option<String>,
        /// dispatch although another member of its group has a job running
        #[arg(long)]
        force: bool,
    },
    /// the project's jobs on every machine, read over ssh: running, done, failed or lost
    #[command(hide = true)]
    Jobs {
        /// return when one of the running jobs ends, printing them all
        #[arg(long)]
        wait: bool,
        /// seconds between reads while waiting
        #[arg(long, default_value_t = 20)]
        every: u64,
        /// stop waiting after this many seconds even when no job ended; 0 waits for one
        #[arg(long, default_value_t = 0)]
        timeout: u64,
        /// every project's jobs, not only this one's
        #[arg(long)]
        all: bool,
    },
    /// commit a finished job's change here, on its branch, with the message it proposed, and clear
    /// the job from the machine it ran on
    #[command(hide = true)]
    Collect {
        id: String,
        /// clear the job without committing anything: for a job that failed, was lost, or waits on
        /// a question
        #[arg(long)]
        discard: bool,
    },
    /// what a public repository must not carry: the owner's private names, read from the server
    #[command(hide = true)]
    Private {
        #[command(subcommand)]
        what: PrivateCmd,
    },
    /// the owner's one-off moves of the data: migrate prints what moving onto the core changes
    Admin {
        #[command(subcommand)]
        what: AdminCmd,
    },
    /// bind this directory to a project by hand
    #[command(hide = true)]
    Bind {
        slug: Option<String>,
        #[arg(long)]
        root: Option<String>,
    },
    /// take a directory (this one by default) off this machine's roots
    #[command(hide = true)]
    Unbind { path: Option<String> },
}

/// `docket dep`: an item's dependencies, added or removed.
#[derive(Subcommand, Debug)]
pub enum DepCmd {
    /// ID depends on each ON as well; refused when one would close a cycle
    Add {
        id: String,
        #[arg(required = true)]
        on: Vec<String>,
        #[arg(long)]
        force: bool,
    },
    /// ID no longer depends on each ON; it resumes when nothing holds it
    Rm {
        id: String,
        #[arg(required = true)]
        on: Vec<String>,
        #[arg(long)]
        force: bool,
    },
}

/// `docket admin`: one-off moves of the data.
#[derive(Subcommand, Debug)]
pub enum AdminCmd {
    /// what moving the project's rows onto the core changes: releases, dependencies, parents,
    /// labels and assignments, with every risky case; refused without --dry-run while a case waits
    /// on a decision
    Migrate {
        /// print every change and risky case and write nothing
        #[arg(long)]
        dry_run: bool,
        /// every project in the server's dump, not only this one
        #[arg(long)]
        all: bool,
        /// an item whose theme is not a release: to its plan's release else the backlog (plan, the
        /// default), or always the backlog, labelled with the theme each way
        #[arg(long, value_parser = ["plan", "backlog"])]
        themes: Option<String>,
    },
    /// carry done and dropped items' leading shas onto rewritten commits through a map of
    /// `old new` full shas, one remapped event each; refused when a new sha is no commit here
    Remap {
        /// the map file, the layout of a filter-repo commit-map
        mapfile: String,
        /// print the count and the first rows and write nothing
        #[arg(long)]
        dry_run: bool,
    },
}

/// `docket private`: the owner's private names kept out of a public repository.
#[derive(Subcommand, Debug)]
pub enum PrivateCmd {
    /// search the tracked files (or the paths given, or the staged change) for every private name;
    /// exits 1 on a hit
    Check {
        /// the change staged for the next commit, not the tracked files
        #[arg(long)]
        staged: bool,
        /// a commit message file, as a commit-msg hook passes it
        #[arg(long, value_name = "FILE")]
        message: Option<String>,
        /// every commit of a git range (`origin/main..HEAD`, `--all`): its message and what it adds
        #[arg(long, value_name = "RANGE", allow_hyphen_values = true)]
        range: Option<String>,
        /// also list the docket item ids code comments cite, for a person to read: an invented
        /// example id looks the same as a citation, so the hooks leave this out
        #[arg(long)]
        ids: bool,
        paths: Vec<String>,
    },
    /// every private name, one a line, for a history rewrite's replace file kept outside the repository
    Terms,
    /// install pre-commit, commit-msg and pre-push hooks that run the check in this checkout
    Hook {
        /// replace hooks that docket did not write
        #[arg(long)]
        force: bool,
    },
}

/// `docket job`: the jobs a lead starts on this machine, here or over ssh.
#[derive(Subcommand, Debug)]
pub enum JobCmd {
    /// start a job for -p SLUG on --branch NAME, which the lead pushed to this machine's checkout:
    /// a worktree beside the checkout, the brief, and the session detached
    Run {
        /// the item the job works
        #[arg(long)]
        id: String,
        #[arg(long, value_parser = crate::job::RUNNERS)]
        runner: String,
        #[arg(long)]
        model: String,
        #[arg(long)]
        effort: Option<String>,
        #[arg(long, value_parser = JOB_ROLES, default_value = "build")]
        role: String,
        /// a repository the item changes, a path under this machine's root or @OWNER/NAME for
        /// another project's; several make a job directory holding a worktree of each; none takes
        /// the checkout fact
        #[arg(long, value_name = "PATH")]
        repo: Vec<String>,
    },
    /// every job on this machine, or the one named: running, done, failed or lost, and its report
    Status { job: Option<String> },
    /// the checkout of -p SLUG on this machine, where a lead pushes a job's branch: the repository
    /// --repo names under this machine's root, else the checkout fact's
    Where {
        #[arg(long, value_name = "PATH")]
        repo: Option<String>,
    },
    /// a finished job's change against the commit it started from, the change inside each
    /// submodule included, for the lead to commit on its own machine
    Diff { job: String },
    /// remove a finished job: its worktree, its branch in the checkout and its record
    Remove {
        job: String,
        /// leave the branch, for a job that ran in the lead's own repository
        #[arg(long)]
        keep_branch: bool,
    },
    /// stop a job's whole process group
    Kill { job: String },
    /// post a finished job's end to the server, from the job's own directory; its wrapper runs it
    #[command(hide = true)]
    Report { dir: std::path::PathBuf },
    /// the last lines of a job's event stream
    Log {
        job: String,
        #[arg(short = 'n', long, default_value_t = 20)]
        n: usize,
    },
}

const GROUPS: [(&str, &[&str]); 3] = [
    (
        "Owner",
        &[
            "status",
            "todo",
            "waiting",
            "questions",
            "research",
            "derived",
            "answer",
            "reply",
            "projects",
            "done",
            "dropped",
            "changelog",
            "audit",
            "graph",
            "groups",
            "skills",
            "releases",
            "areas",
            "labels",
            "admin",
        ],
    ),
    (
        "Agent",
        &[
            "next", "complex", "show", "log", "new", "add", "start", "unclaim", "close", "drop",
            "reopen", "wait", "dep", "resume", "ask", "decide", "rate", "priority", "edit",
            "label", "link", "parent", "search", "similar", "deps", "files", "check", "stale",
            "squash",
        ],
    ),
    ("Lead", &["wip", "machines"]),
];

/// The command with its verbs listed under Owner, Agent and Lead; plumbing stays out of the list.
pub fn command() -> clap::Command {
    use std::fmt::Write;
    let cmd = Cli::command();
    let mut text = String::new();
    for (heading, verbs) in GROUPS {
        let _ = writeln!(text, "{heading}:");
        for v in verbs {
            let about = cmd
                .find_subcommand(v)
                .and_then(|c| c.get_about())
                .map(ToString::to_string)
                .unwrap_or_default();
            let about = about.lines().next().unwrap_or("").to_string();
            let _ = writeln!(text, "  {v:<10} {about}");
        }
        text.push('\n');
    }
    text.push_str("`docket help --all` lists the plumbing verbs too.");
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    let mut cmd = cmd
        .override_usage("docket [OPTIONS] [COMMAND]")
        .after_help(text)
        .help_template("{about}\n\n{usage-heading} {usage}\n\n{after-help}\n\nOptions:\n{options}");
    for n in names {
        cmd = cmd.mut_subcommand(n, |c| c.hide(true));
    }
    cmd
}

/// The command with every verb listed in one block, for `docket help --all`.
pub fn full_help() -> String {
    let mut cmd = Cli::command();
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    for n in names {
        cmd = cmd.mut_subcommand(n, |c| c.hide(false));
    }
    cmd.render_help().to_string()
}

#[cfg(test)]
#[path = "tests/args.rs"]
mod tests;

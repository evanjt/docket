//! The command line: every verb, its arguments and the flags taken before or after it.

use clap::{Args, Parser, Subcommand};

const COMPLEXITIES: [&str; 3] = ["high", "medium", "low"];
const PRIORITIES: [&str; 4] = ["critical", "high", "normal", "low"];
const TURNS: [&str; 2] = ["agent", "user"];
const STATES: [&str; 4] = ["open", "done", "dropped", "any"];
const RUNNERS: [&str; 3] = ["codex", "claude", "remote"];
const ROLES: [&str; 4] = ["build", "rebase", "review", "plan"];
const WAITS: [&str; 2] = ["item", "condition"];
const QUEUE_ROLES: [&str; 3] = ["plan", "work", "audit"];
const KINDS: [&str; 8] = [
    "work", "decision", "research", "audit", "story", "concept", "idea", "package",
];

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
    /// bare command: seconds between samples
    #[arg(long, default_value_t = 5)]
    pub every: u64,
    /// bare command: seconds of movements shown
    #[arg(long, default_value_t = 86_400)]
    pub window: u64,
    /// bare command: closes the pace is read from
    #[arg(long, default_value_t = 20)]
    pub recent: u64,
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
    /// only items carrying this theme
    #[arg(long)]
    pub theme: Option<String>,
    /// only items a plan opened, at any depth
    #[arg(long, value_name = "ID")]
    pub under: Option<String>,
    /// only what this role takes: plan (new plans, investigations, decided questions), work
    /// (tickets), audit (plans whose tickets are all closed)
    #[arg(long, value_parser = QUEUE_ROLES)]
    pub role: Option<String>,
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
        #[arg(long)]
        theme: Option<String>,
    },
    /// the owner's queue: open items on the user's turn
    Todo,
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
    /// open decisions, by theme
    #[command(alias = "q")]
    Questions {
        #[arg(long)]
        theme: Option<String>,
    },
    /// decided questions that owe work items
    Research,
    /// decisions agents derived, with their basis: the owner's digest
    Derived { n: Option<i64> },
    /// recently done, newest first
    Done(Recent),
    /// recently dropped, newest first
    Dropped(Recent),
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
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        group: Option<String>,
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
    Release {
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
        #[arg(long)]
        force: bool,
    },
    /// done, under a sha or what it opened; releases waiters
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
    Reopen { id: String, why: String },
    /// park an item behind another, or until a condition
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
    /// clear a wait by hand
    Resume { id: String, note: Option<String> },
    /// hand an item to the owner: their turn, with what is needed
    #[command(aliases = ["manual", "park"])]
    Ask {
        id: String,
        note: String,
        #[arg(long)]
        force: bool,
    },
    /// hand it back to the agents, with what happened
    Reply { id: String, note: String },
    /// record a decision on a question; releases what waited on it
    Answer {
        id: String,
        decision: String,
        /// the decision, central idea or practice an agent derived it from
        #[arg(long, value_name = "BASIS")]
        derived: Option<String>,
    },
    /// record a choice made on an item by best practice, with its basis
    Decide {
        id: String,
        choice: String,
        /// the decision, central idea or practice it rests on
        #[arg(long, required = true)]
        basis: String,
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
        #[arg(long = "set", value_name = "FIELD=VALUE")]
        set: Vec<String>,
        #[arg(long, value_name = "TEXT")]
        append: Option<String>,
        #[arg(long, value_name = "FILE|-")]
        body: Option<String>,
    },
    /// A related B, or A opened-by B; several A at once
    Link {
        /// A... related|opened B
        #[arg(required = true, num_args = 3.., value_name = "A KIND B")]
        words: Vec<String>,
        #[arg(long)]
        remove: bool,
    },
    /// where a plan, story, package, concept, idea, group or theme stands
    Audit {
        id: Option<String>,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        theme: Option<String>,
        /// open items that belong to no concept
        #[arg(long)]
        orphans: bool,
    },
    /// items, links and cited files, as JSON or Graphviz dot
    Graph {
        #[arg(long)]
        dot: bool,
        /// leave out the cited files
        #[arg(long)]
        no_files: bool,
    },
    /// add a key to the project's matrix, or change what it means
    Key {
        key: String,
        #[arg(value_parser = KINDS)]
        kind: String,
        meaning: String,
        #[arg(long, value_parser = TURNS)]
        turn: Option<String>,
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
        #[arg(long)]
        theme: Option<String>,
        #[arg(long)]
        without_theme: Option<String>,
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
    Reindex,
    /// citations that no longer resolve
    Stale {
        #[arg(long)]
        open_only: bool,
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
    },
    /// the docket block in the AGENTS.md at the project's root: install writes it, diff lists what
    /// install would change
    Instructions {
        #[arg(value_parser = ["install", "diff"], default_value = "diff")]
        what: String,
        /// install without asking, for a run with no terminal
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// give an item the loop parked or sent back a fresh start
    Retry { id: String, note: Option<String> },
    /// the machines jobs run on, this one marked
    Machines,
    /// set or remove a machine, on the owner's key; set changes only the fields given
    Machine {
        #[arg(value_parser = ["set", "remove"])]
        what: String,
        /// the host its key names
        name: String,
        /// the ssh address the other machines reach it at
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
    },
    /// the project's lead claim: show who leads, or take, renew or give it back
    Lead {
        #[arg(value_parser = ["show", "take", "renew", "give"], default_value = "show")]
        what: String,
        /// the session holding the claim on this host; default: the branch acting
        #[arg(long)]
        session: Option<String>,
    },
    /// bind this directory to a project by hand
    Bind {
        slug: Option<String>,
        #[arg(long)]
        root: Option<String>,
    },
}

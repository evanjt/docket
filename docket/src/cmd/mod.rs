//! Each verb to its command.

pub mod admin;
pub mod audit;
pub mod dispatch;
pub mod instructions;
pub mod job;
pub mod lead;
pub mod lists;
pub mod machines;
pub mod private;
pub mod releases;
pub mod show;
pub mod skills;
pub mod status;
pub mod write;

use docket_core::api::{
    AnswerRequest, AskRequest, DecideRequest, DepRequest, DropRequest, KeyRequest, PriorityRequest,
    RateRequest, ReleaseRequest, ReopenRequest, ReplyRequest, ResumeRequest, RetryRequest,
    StartRequest, WaitRequest,
};

use crate::args::{Cmd, DepCmd, Queue};
use crate::ctx::Ctx;
use crate::fail::Result;

/// The verb run, and the exit code it ends on.
///
/// # Errors
/// The command's refusal.
pub fn run(ctx: &mut Ctx, cmd: Option<&Cmd>) -> Result<i32> {
    let Some(cmd) = cmd else {
        return status::status(ctx);
    };
    match cmd {
        Cmd::Status => status::status(ctx),
        Cmd::Next(q) => lists::next(ctx, q),
        Cmd::Complex { n, theme } => {
            let q = Queue {
                n: *n,
                complexity: Some("high".into()),
                theme: theme.clone(),
                ..Queue::default()
            };
            lists::next(ctx, &q)
        }
        Cmd::Todo(q) => lists::todo(ctx, q),
        Cmd::Wip { host } => lists::wip(ctx, host.as_ref()),
        Cmd::Waiting { on } => lists::waiting(ctx, on.as_ref()),
        Cmd::Questions { theme } => lists::questions(ctx, theme.as_ref()),
        Cmd::Research => lists::research(ctx),
        Cmd::Derived { n } => lists::derived(ctx, *n),
        Cmd::Done(r) => lists::recent(ctx, r, "done"),
        Cmd::Dropped(r) => lists::recent(ctx, r, "dropped"),
        Cmd::Groups { name } => lists::groups(ctx, name.as_ref()),
        Cmd::Show { id } => show::show(ctx, id),
        Cmd::Log { id } => show::log(ctx, id),
        Cmd::Projects => show::projects(ctx),
        Cmd::Search {
            words,
            key,
            state,
            n,
            raw,
            theme,
            without_theme,
        } => lists::search(
            ctx,
            words,
            key.as_ref(),
            state,
            *n,
            *raw,
            theme.as_ref(),
            without_theme.as_ref(),
        ),
        Cmd::Similar { id, n, state } => lists::similar(ctx, id, *n, state),
        Cmd::Deps { id } => show::deps(ctx, id),
        Cmd::Files { prefix, state } => lists::files(ctx, prefix, state),
        Cmd::Graph { dot, no_files } => show::graph(ctx, *dot, *no_files),
        Cmd::Audit {
            id,
            group,
            theme,
            orphans,
        } => audit::audit(
            ctx,
            &audit::Target {
                id: id.as_ref(),
                group: group.as_ref(),
                theme: theme.as_ref(),
                orphans: *orphans,
            },
        ),
        Cmd::Check { deep } => status::check(ctx, *deep),
        Cmd::Stale { open_only } => audit::stale(ctx, *open_only),
        Cmd::Bind { slug, root } => write::bind(ctx, slug.as_ref(), root.as_ref()),
        Cmd::Skills {
            what,
            key,
            value,
            claude,
            codex,
            yes,
            all_projects,
        } => skills::skills(
            ctx,
            what.as_ref(),
            key.as_ref(),
            value.as_ref(),
            skills::Install {
                claude: *claude,
                codex: *codex,
                yes: *yes,
            },
            *all_projects,
        ),
        Cmd::Instructions { what, yes } => instructions::instructions(ctx, what, *yes),
        Cmd::Machines
        | Cmd::Machine { .. }
        | Cmd::Lead { .. }
        | Cmd::Dispatch { .. }
        | Cmd::Jobs { .. }
        | Cmd::Collect { .. } => run_lead(ctx, cmd),
        Cmd::Private { what } => private::private(ctx, what),
        Cmd::Admin { what } => admin::admin(ctx, what),
        _ => run_write(ctx, cmd),
    }
}

/// The machines and the lead claim, which a lead reads and holds.
fn run_lead(ctx: &mut Ctx, cmd: &Cmd) -> Result<i32> {
    match cmd {
        Cmd::Machines => machines::machines(ctx),
        Cmd::Machine {
            what,
            name,
            ssh,
            slots,
            runners,
            note,
            path,
        } => machines::machine(
            ctx,
            &machines::set_request(
                name,
                ssh.as_ref(),
                *slots,
                runners.as_deref(),
                note.as_ref(),
                path.as_ref(),
                what == "remove",
            ),
        ),
        Cmd::Lead { what, session } => lead::lead(ctx, what, session.as_ref()),
        Cmd::Dispatch {
            id,
            on,
            runner,
            model,
            effort,
            role,
            force,
        } => dispatch::dispatch(
            ctx,
            &dispatch::Ask {
                id,
                on: on.as_deref(),
                runner: runner.as_deref(),
                model: model.as_deref(),
                effort: effort.as_deref(),
                role: role.as_deref(),
                force: *force,
            },
        ),
        Cmd::Jobs {
            wait,
            every,
            timeout,
            all,
        } => dispatch::jobs(ctx, *wait, *every, *timeout, *all),
        Cmd::Collect { id, discard } => dispatch::collect(ctx, id, *discard),
        _ => unreachable!("run_lead is given only the machine, lead and dispatch commands"),
    }
}

fn opt(v: Option<&String>) -> Option<String> {
    v.filter(|s| !s.is_empty()).cloned()
}

#[allow(clippy::too_many_lines)]
fn run_write(ctx: &mut Ctx, cmd: &Cmd) -> Result<i32> {
    match cmd {
        Cmd::New {
            key,
            title,
            body,
            turn,
            complexity,
            priority,
            theme,
            release,
            group,
        } => write::new(
            ctx,
            &write::New {
                key,
                title,
                body: body.as_ref(),
                turn: turn.as_ref(),
                complexity: complexity.as_ref(),
                priority: priority.as_ref(),
                theme: theme.as_ref(),
                release: release.as_ref(),
                group: group.as_ref(),
            },
        ),
        Cmd::Add {
            title,
            key,
            body,
            from,
            release,
        } => write::add(
            ctx,
            title,
            key.as_ref(),
            body.as_ref(),
            from.as_ref(),
            release.as_ref(),
        ),
        Cmd::Start {
            id,
            force,
            runner,
            job,
            model,
            on,
            role,
        } => {
            let req = StartRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                runner: opt(runner.as_ref()),
                job: opt(job.as_ref()),
                model: opt(model.as_ref()),
                on: opt(on.as_ref()),
                role: opt(role.as_ref()),
                effort: None,
            };
            write::start(ctx, &req)
        }
        Cmd::Unclaim {
            id,
            note,
            bounce,
            rebase,
            runner,
            model,
            outcome,
            force,
        } => {
            let req = ReleaseRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                note: opt(note.as_ref()),
                bounce: *bounce,
                rebase: opt(rebase.as_ref()),
                runner: opt(runner.as_ref()),
                model: opt(model.as_ref()),
                outcome: opt(outcome.as_ref()),
            };
            write::release(ctx, &req)
        }
        Cmd::Close {
            id,
            resolution,
            gates,
            runner,
            model,
            force,
        } => write::close(
            ctx,
            &write::Close {
                id,
                resolution: resolution.as_ref(),
                gates: gates.as_ref(),
                runner: runner.as_ref(),
                model: model.as_ref(),
                force: *force,
            },
        ),
        Cmd::Drop {
            id,
            why,
            superseded_by,
            force,
        } => {
            let req = DropRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                why: opt(why.as_ref()),
                superseded_by: opt(superseded_by.as_ref()),
            };
            write::drop(ctx, &req)
        }
        Cmd::Reopen { id, why } => {
            let req = ReopenRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                why: why.clone(),
            };
            write::moved(ctx, "reopen", &req)
        }
        Cmd::Wait {
            id,
            on,
            until,
            force,
        } => {
            let req = WaitRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                on: opt(on.as_ref()),
                until: opt(until.as_ref()),
            };
            write::moved(ctx, "wait", &req)
        }
        Cmd::Resume { id, note, force } => {
            let req = ResumeRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                note: opt(note.as_ref()),
            };
            write::moved(ctx, "resume", &req)
        }
        Cmd::Dep { what } => {
            let (id, on, force, remove) = match what {
                DepCmd::Add { id, on, force } => (id, on, force, false),
                DepCmd::Rm { id, on, force } => (id, on, force, true),
            };
            let req = DepRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                on: on.clone(),
                remove,
            };
            write::moved(ctx, "dep", &req)
        }
        Cmd::Ask {
            id,
            note,
            need,
            force,
        } => {
            let req = AskRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                note: note.clone(),
                need: need.clone(),
            };
            write::ask(ctx, &req)
        }
        Cmd::Reply { id, note } => {
            let req = ReplyRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                note: note.clone(),
            };
            write::moved(ctx, "reply", &req)
        }
        Cmd::Answer {
            id,
            decision,
            derived,
            carried_by,
        } => {
            let req = AnswerRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                decision: decision.clone(),
                derived: derived.clone(),
                carried_by: carried_by.clone(),
            };
            write::answer(ctx, &req)
        }
        Cmd::Decide { id, choice, basis } => {
            let req = DecideRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                choice: choice.clone(),
                basis: basis.clone(),
            };
            write::moved(ctx, "decide", &req)
        }
        Cmd::Rate { id, level } => {
            let req = RateRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                level: level.clone(),
            };
            write::moved(ctx, "rate", &req)
        }
        Cmd::Priority { ids, tier } => {
            let req = PriorityRequest {
                common: ctx.common(false)?,
                ids: ids.clone(),
                tier: tier.clone(),
            };
            write::priority(ctx, &req)
        }
        Cmd::Edit {
            id,
            set,
            release,
            append,
            body,
        } => write::edit(
            ctx,
            id,
            set,
            release.as_ref(),
            append.as_ref(),
            body.as_ref(),
        ),
        Cmd::Releases {
            action,
            name,
            to,
            move_open_to,
            target,
            note,
            all,
        } => releases::releases(
            ctx,
            &releases::Releases {
                action: action.as_deref(),
                name: name.as_deref(),
                to: to.as_deref(),
                move_open_to: move_open_to.as_deref(),
                target: target.as_deref(),
                note: note.as_deref(),
                all: *all,
            },
        ),
        Cmd::Link { words, remove } => write::link(ctx, words, *remove),
        Cmd::Parent { words, none } => write::parent(ctx, words, *none),
        Cmd::Key {
            key,
            kind,
            meaning,
            turn,
        } => {
            let req = KeyRequest {
                common: ctx.common(false)?,
                key: key.clone(),
                kind: kind.clone(),
                meaning: meaning.clone(),
                turn: opt(turn.as_ref()),
            };
            write::key(ctx, &req)
        }
        Cmd::Reindex => write::reindex(ctx),
        Cmd::Retry { id, note } => {
            let req = RetryRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                note: opt(note.as_ref()),
            };
            write::moved(ctx, "retry", &req)
        }
        _ => Ok(0),
    }
}

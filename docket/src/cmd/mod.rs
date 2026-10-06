//! Each verb to its command.

pub mod admin;
pub mod areas;
pub mod audit;
pub mod dispatch;
pub mod instructions;
pub mod job;
pub mod labels;
pub mod lead;
pub mod lists;
pub mod machines;
pub mod private;
pub mod published;
pub mod releases;
pub mod show;
pub mod skills;
pub mod squash;
pub mod status;
pub mod write;

use docket_core::api::{
    AnswerRequest, AskRequest, DecideRequest, DepRequest, DropRequest, PriorityRequest,
    RateRequest, ReleaseRequest, ReopenRequest, ReplyRequest, ResumeRequest, StartRequest,
    WaitRequest,
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
        Cmd::Complex { n, label } => {
            let q = Queue {
                n: *n,
                complexity: Some("high".into()),
                label: label.clone(),
                ..Queue::default()
            };
            lists::next(ctx, &q)
        }
        Cmd::Todo(q) => lists::todo(ctx, q),
        Cmd::Wip { host } => lists::wip(ctx, host.as_ref()),
        Cmd::Waiting { on } => lists::waiting(ctx, on.as_ref()),
        Cmd::Questions { label } => lists::questions(ctx, label.as_ref()),
        Cmd::Research => lists::research(ctx),
        Cmd::Derived { n } => lists::derived(ctx, *n),
        Cmd::Done(r) => lists::recent(ctx, r, "done"),
        Cmd::Changelog { release, plans } => lists::changelog(ctx, release.as_ref(), *plans),
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
            label,
            without_label,
        } => lists::search(
            ctx,
            words,
            key.as_ref(),
            state,
            *n,
            *raw,
            label.as_ref(),
            without_label.as_ref(),
        ),
        Cmd::Similar { id, n, state } => lists::similar(ctx, id, *n, state),
        Cmd::Deps { id } => show::deps(ctx, id),
        Cmd::Files { prefix, state } => lists::files(ctx, prefix, state),
        Cmd::Graph { dot, no_files } => show::graph(ctx, *dot, *no_files),
        Cmd::Audit {
            id,
            group,
            label,
            area,
        } => audit::audit(
            ctx,
            &audit::Target {
                id: id.as_ref(),
                group: group.as_ref(),
                label: label.as_ref(),
                area: area.as_ref(),
            },
        ),
        Cmd::Check { deep } => status::check(ctx, *deep),
        Cmd::Stale { open_only, prune } => audit::stale(ctx, *open_only, *prune),
        Cmd::Squash { messages, cap } => squash::squash(ctx, messages.as_deref(), *cap),
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
            area,
            parent,
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
                area: area.as_ref(),
                parent: parent.as_ref(),
            },
        ),
        Cmd::Add {
            title,
            key,
            body,
            from,
            release,
            area,
            parent,
        } => write::add(
            ctx,
            &write::Add {
                title,
                key: key.as_ref(),
                body: body.as_ref(),
                from: from.as_ref(),
                release: release.as_ref(),
                area: area.as_ref(),
                parent: parent.as_ref(),
            },
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
        Cmd::Reopen {
            id,
            why,
            area,
            release,
            carry,
            force,
        } => {
            let req = ReopenRequest {
                common: ctx.common(*force)?,
                id: id.clone(),
                why: why.clone(),
                area: area.clone(),
                release: release.clone(),
                carry: *carry,
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
        Cmd::Resume { id, note } => {
            let req = ResumeRequest {
                common: ctx.common(false)?,
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
        Cmd::Decide {
            id,
            choice,
            basis,
            area,
            about,
        } => {
            let req = DecideRequest {
                common: ctx.common(false)?,
                id: id.clone(),
                choice: choice.clone().unwrap_or_default(),
                basis: basis.clone(),
                area: area.clone(),
                about: about.clone(),
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
            carry,
            area,
            append,
            body,
        } => write::edit(
            ctx,
            &write::Edit {
                id,
                set,
                release: release.as_ref(),
                carry: *carry,
                area: area.as_ref(),
                append: append.as_ref(),
                body: body.as_ref(),
            },
        ),
        Cmd::Areas {
            action,
            name,
            about,
            rename,
            priority,
            to,
        } => areas::areas(
            ctx,
            &areas::Areas {
                action: action.as_deref(),
                name: name.as_deref(),
                about: about.as_deref(),
                rename: rename.as_deref(),
                priority: priority.as_deref(),
                to: *to,
            },
        ),
        Cmd::Label {
            action,
            id,
            name,
            about,
        } => labels::label(ctx, action, id, name, about.as_deref()),
        Cmd::Labels => labels::list(ctx),
        Cmd::Releases {
            action,
            name,
            to,
            move_open_to,
            carry,
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
                carry: *carry,
                target: target.as_deref(),
                note: note.as_deref(),
                all: *all,
            },
        ),
        Cmd::Link { words, remove } => write::link(ctx, words, *remove),
        Cmd::Parent { words, none } => write::parent(ctx, words, *none),
        Cmd::Reindex => write::reindex(ctx),
        _ => Ok(0),
    }
}

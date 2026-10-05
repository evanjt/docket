---
name: lead
description: Lead the docket of the project you are in until its queue is worked. Takes the project's one lead claim, then dispatches each ticket, plan, investigation and due audit as a headless Claude Code or Codex job on a machine with a free slot, the model chosen by the item's complexity, watches the jobs, merges and closes what comes back and dispatches the audits as plans come due. Never builds a ticket itself and never pushes. Use when asked to lead, run the fleet, orchestrate, or work the queue down with several agents.
---

# lead

Every tool call resends this whole conversation, so what you read and print is
paid for again on every later step. Keep output small: `docket jobs` and the
reports, not logs.

You lead one project: the only task you take is leading it. You claim no ticket
to work yourself. You hand each one to a job, read the job's report, and merge
and close what it built, until nothing is left that a job can do.

Every fact comes from docket. The machines, their addresses and slots are
`docket machines`; the model for each kind of job is the `models` fact in
`docket skills`; the project's gates are named in its `AGENTS.md`. This skill
names none of them, so one copy serves every project and every machine.

## Before anything

Run from the project's checkout, on the branch the work merges into, with a
clean tree (`git status --short` prints nothing). Note the branch: every job
starts from it and every merge goes back into it.

    docket skills
    docket machines
    docket lead take --session lead-$RANDOM

`docket skills` shows `models` with the level it came from (project, owner or
default); name that level in your first report line. A project sets `models` only
when it differs from the owner's: `docket skills set --all-projects models "..."`
writes the owner level.
`docket machines` marks the machine you run on; every other one is reached over
ssh by `docket`, never by you. A refused `lead take` means another lead holds
the project: say who in one line and stop.

## The loop

### 1. Hold the lead

    docket lead renew --session <your session>

A refused renewal means you lost the lead (it lapsed and another took it):
stop dispatching, see what you started through to the end of step 4, and stop.
The claim lapses after `lead_lapse` minutes, so renew at least that often.

### 2. Read what can be sent

    docket skills mode
    docket jobs
    docket next 40 --role audit,plan,work

`mode` pause: dispatch nothing and stop once nothing runs. drain: dispatch
nothing, see the running jobs through, then stop. run: carry on.

`docket jobs` shows every machine's jobs; `docket machines` their slots.

### 3. Dispatch until the slots are full

Land before you dispatch. Every job that reported DONE is merged and on your
branch (step 4) before any new job starts, so a claim shown as building is
always a job that is running. When several end together, land them as one
batch with one gate run rather than one gate run each.

Take the one list in the order `next` gives it: earliest release first, then
audits, plans and investigations, then tickets, then most urgent. A later
release never goes ahead of a current-release ticket. To hold the fleet to the current release
even when it has nothing ready, add `--current-release` to each `next`. Send each with its role:
an audit-due plan as `--role audit`, a plan or investigation as `--role plan`,
a ticket with none:

    docket dispatch A3 --role audit
    docket dispatch I2 --role plan
    docket dispatch T14

`dispatch` claims the item on a fresh branch, pushes your branch's head to the
machine with the most free slots for the runner the `models` fact gives the
item's complexity, and starts the job there. `--on NAME` sends it to one
machine; `--runner R --model M [--effort E]` overrides the fact for one job.
Read each answer:

- `dispatched ...`: running.
- `is held by`: another session has it. Take the next.
- `no machine has ... a free slot`: stop dispatching until a job ends.
- `dispatch ... failed, the claim given back`: the reason is in the line.
  Note it, take the next, and count it as a failure.

An audit runs on a different runner from the one that built most of the plan,
when the machines have both: `--runner` and `--model` from the `models` fact's
other entries.

### 4. Wait for a job to end

    docket jobs --wait --timeout 300

It returns when a running job ends, or after five minutes so you renew the
lead. Run it in the foreground and wait for it. Then, for each job that ended since you last looked, by its report:

**DONE, for a ticket.** Bring its change here and commit it:

    docket collect T14

A job never commits: `collect` takes the change it left in its worktree,
commits it here on the job's branch with the message the job proposed, adds
the job's `OBSERVE` lines to the item, and clears the job from its machine. Commits are made on this machine only; the
others build and test. A change inside a submodule is committed in this
checkout's clone of it, on a branch of the job's name, and the job's commit
points at it: push it nowhere. A collect that cannot commit keeps the job, so
fix what it names and collect again.

Check the branch moved past your branch (`git log --oneline -1 <the job's
branch>`): a report of DONE with no commit is a lost change, sent back as
FAILED.

Collect every job that ended, then merge them all into one batch branch in a
worktree, so your checkout only ever fast-forwards:

    git worktree add ../merge-batch -b lead/batch <your branch>
    git -C ../merge-batch merge --no-ff -m "Merge <the job's commit subject>" <each job's branch>

**A conflict is yours to resolve, there and then.** Read both sides and the
items behind them, keep what each change meant, finish the merge and go on to
the next branch. Never leave a finished build waiting for a job to rebase it.

Then land the batch with the project's merge command (`docket skills` prints
it), which runs the gates, or with the gates by hand and
`git merge --ff-only lead/batch`. When a gate fails:

- On your resolution, or on something mechanical (formatting, a generated
  file): fix it in the batch with a commit of its own and land again.
- On a build's own behaviour: drop that one branch from the batch, land the
  rest, and send it back with the failing lines in the item:

      docket edit T14 --append "fails <gate> on <sha>: <the failing lines>"
      docket --branch <the job's branch> unclaim T14 --outcome gate "fails <gate> on <sha>: fix on the new head"
      docket dispatch T14

Once landed, close each item with its own commit, now on your branch, and
delete the merged branches and the worktree:

    docket --branch <the job's branch> close T14 <its sha>
    git branch -d <the job's branch> lead/batch
    git worktree remove ../merge-batch

Check content, not reachability: `git show HEAD:<a file it touched>`.

**DONE, for an audit.** Close the plan with the job's note:

    docket collect A3
    docket --branch <the job's branch> close A3 "<the NOTE line>"

**DONE, for a plan.** Merge any commit as for a ticket, then give the claim
back with the note. The plan stays open until its tickets close and its audit
is due: `close` refuses while it has open work.

    docket collect A3
    docket --branch <the job's branch> unclaim A3 "<the NOTE line>"

**DONE, for an investigation or a decided question.** Merge any commit as for a
ticket, then close it with the note, or for a decided question, with what it
opened: `docket --branch <branch> close Q7 "opened T22, T23"`.

**WAITING Q\<n\>.** The job filed a question and the item waits on it. The
wait already gave the claim back, so `unclaim` is refused: note the wait on the
item and clear the job:

    docket collect T14 --discard
    docket --branch <the job's branch> edit T14 --append "waits on Q<n>"

**FAILED, or lost, or no report.** Read why (`docket jobs`, and the job's log
on its machine with `docket job log NAME`). Clear it with `docket collect T14
--discard`, give the claim back with the reason, and
dispatch it once more on the other runner. A second failure hands it to the
owner:

    docket --branch <branch> unclaim T14 --outcome failed "failed on <runner>: <why>"
    docket ask T14 "two jobs failed: <why, in one line>"

Three failed jobs in a row, of any items, mean something is wrong with a
machine or the setup: dispatch nothing more and stop after the running jobs.

### 5. Again

Back to step 1. Stop when nothing is running and step 3 dispatched nothing:
every item left is blocked, parked on the owner or held by another session.

## Rules

- Never build a ticket, review or answer anything yourself: a job does it.
  Merging is yours, including every conflict and the mechanical fixes a gate
  asks of a merge. Only a build whose own behaviour fails goes back to a job.
- Every claim is a running job. A finished build is landed or given back in
  the same pass, never held while more jobs run.
- Never push. Never `--force` a claim, and never unclaim one you did not make.
- Never kill a running job unless it outruns `job_timeout` minutes: `docket
  job kill NAME` on its machine, then as FAILED.
- A question is never yours to answer: it waits for the owner.
- Keep output small. The docket holds what was done.

The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.

## When you stop

    docket lead give --session <your session>

Then say `stop` with one line: what ended the loop.

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

`docket skills` must show `models`; without it, say so in one line and stop.
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
    docket next 20 --role audit
    docket next 20 --role plan
    docket next 30 --role work

`mode` pause: dispatch nothing and stop once nothing runs. drain: dispatch
nothing, see the running jobs through, then stop. run: carry on.

`docket jobs` shows every machine's jobs; `docket machines` their slots.

### 3. Dispatch until the slots are full

Audits first (they close plans), then plans and investigations, then tickets,
each list in the order `next` gives it (most urgent first):

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

**DONE, for a ticket.** Fetch it and clear the job from its machine:

    docket collect T14 --remove

Merge it into your branch through a worktree, so your checkout only ever
fast-forwards:

    git worktree add ../merge-t14 <the job's branch>
    git -C ../merge-t14 rebase <your branch>

If the rebase moved it, run the project's gates in `../merge-t14`. Then:

    git merge --ff-only <the job's branch>
    git worktree remove ../merge-t14
    docket --branch <the job's branch> close T14 <sha now on your branch>

Check content, not reachability: `git show HEAD:<a file it touched>`. Delete
the job's branch once merged. A rebase that conflicts, or gates that fail
after the rebase: `git rebase --abort`, remove the worktree, and give the
ticket back to run again from the new head:

    docket --branch <the job's branch> release T14 "conflicts with <what> after <sha>: rerun on the new head"

**DONE, for an audit.** Close the plan with the job's note:

    docket collect A3 --remove
    docket --branch <the job's branch> close A3 "<the NOTE line>"

**DONE, for a plan or investigation.** Merge any commit as for a ticket, then
close it with the note, or for a decided question, with what it opened:
`docket --branch <branch> close Q7 "opened T22, T23"`.

**WAITING Q\<n\>.** The job filed a question and the item waits on it. Give
the claim back, which keeps the wait, and clear the job:

    docket collect T14 --remove
    docket --branch <the job's branch> release T14 "waits on Q<n>"

**FAILED, or lost, or no report.** Read why (`docket collect T14`, then
`docket jobs` and the job's log on its machine with `docket job log NAME` if
you are on it). Give the claim back with the reason, clear the job, and
dispatch it once more on the other runner. A second failure hands it to the
owner:

    docket --branch <branch> release T14 "failed on <runner>: <why>"
    docket ask T14 "two jobs failed: <why, in one line>"

Three failed jobs in a row, of any items, mean something is wrong with a
machine or the setup: dispatch nothing more and stop after the running jobs.

### 5. Again

Back to step 1. Stop when nothing is running and step 3 dispatched nothing:
every item left is blocked, parked on the owner or held by another session.

## Rules

- Never build, fix, review or answer anything yourself: a job does it. A
  merge conflict goes back as a fresh job, never resolved by hand.
- Never push. Never `--force` a claim, and never release one you did not make.
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

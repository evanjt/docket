---
name: lead
description: Lead the docket of the project you are in until its queue is worked. Takes the project's one lead claim, then dispatches each ticket, plan, investigation and due audit as a headless Claude Code or Codex job on a machine with a free slot, the model chosen by the item's complexity, watches the jobs, merges and closes what comes back and dispatches the audits as plans come due. Never builds a ticket itself and never pushes. Use when asked to lead, run the fleet, orchestrate, or work the queue down with several agents.
---

# lead

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

A project whose root holds several sibling repositories is led from under its
root. Each item carries the repository it changes, a path under the root
(`docket new --repo PATH`, `docket edit ID --repo PATH`, or its plan's); give
`--repo` once for each repository the item changes, and `@OWNER/NAME` for
another project's root; one naming none takes the `checkout` fact. `dispatch` and `collect` take the
base from and commit in that repository, on each machine the one under its own
root, and you merge the job's branch there. `docket next --repo PATH` lists one
repository's items. An item can name several repositories, each a `repo:`
label (`docket label add ID repo:PATH`): its job gets one directory laid out
like the root, a worktree of each on one branch, and is placed only on a
machine that has them all. `collect` lands every repository or none, and
prints `REPO@SHA` for each: merge the branch in each repository and close
with those words. A job's `OBSERVE [REPO] text` line keeps its repository when
`collect` appends it; when you file a ticket from it, give the ticket that
`--repo`.

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

Read `docket skills gate`. When `gate` is unset, `docket land` refuses, so set it
before the loop: `docket skills set gate "..."` to a shell command that runs the
project's `gates` fact in a fresh worktree, with what the `traps` fact says a
fresh worktree needs (installing dependencies, the test database). Say in your
first report line that the setting is derived from the `gates` fact.

## The loop

### 1. Hold the lead

    docket lead renew --session <your session>

A refused renewal means you lost the lead (it lapsed and another took it):
stop dispatching, see what you started through to the end of step 4, and stop.
The claim lapses after `lead_lapse` minutes, so renew at least that often.

### 2. Read what can be sent

    docket skills mode
    docket jobs
    docket next 40

`mode` pause: dispatch nothing and stop once nothing runs. drain: dispatch
nothing, see the running jobs through, then stop. run: carry on.

Each row carries the role it is taken in. A `Halt:` line above the rows means the
last three ended attempts all failed: dispatch nothing more and stop after the
running jobs. A `Stall:` line means a slot is free and an item is ready, or a build waits
to land with no gate run: dispatch or `docket land` it in this pass.

`docket jobs` shows every machine's jobs; `docket machines` their slots.

### 3. Dispatch until the slots are full

A slot never waits for a landing. Every job that reported DONE goes to
`docket land` at once (step 4) and dispatching goes on while the gate runs. No
finished build waits longer than the gate run in progress: it goes into the
next batch, merged onto the head the previous batch left, so it does not go
stale as the branch moves.

An item whose Evidence or Fix needs the change of another claimed or collected
item is not dispatched until that change has landed: its job would build on a
base that lacks it, and fail or conflict. Mark it `docket wait ID --on OTHER`;
where `wait` refuses (a later release, say), note it on the item and hold it
by hand. Dispatch it the moment the other change lands.

Take the one list in the order `next` gives it: earliest release first, then
most urgent, then audits, plans and investigations, then tickets, then the item
that unblocks the most, then the oldest. A later
release never goes ahead of a current-release ticket. To hold the fleet to the current release
even when it has nothing ready, add `--current-release` to each `next`. To work one area whole, add `--area NAME` to each `next`. `dispatch` takes each item's role from its row
in `next`, so name none:

    docket dispatch A3
    docket dispatch I2
    docket dispatch T14

`dispatch` claims the item on a fresh branch, pushes your branch's head to the
machine with the most free slots for the runner the `models` fact gives the
item's complexity, and starts the job there. `--on NAME` sends it to one
machine; `--runner R --model M [--effort E]` overrides the fact for one job.
Read each answer:

- `dispatched ...`: running.
- `is held by`: another session has it. Take the next.
- `is in group G, whose member ... runs`: a group is one job's work. Leave the
  rest of G until that job is collected, then dispatch the next member, whose
  worktree starts from the first member's landed change. `--force` only when
  the members touch nothing in common.
- `no machine has ... a free slot`: stop dispatching until a job ends.
- `reported a usage limit` or `every runner ... reported a usage limit`: the
  runner is out until the reset the line names, and dispatch already uses the
  `models` fact's entry on the other runner when there is one. With none, stop
  dispatching until that reset. It is not a failure.
- `dispatch ... failed, the claim given back`: the reason is in the line.
  Note it and take the next: the server counts it.

An audit runs on the `models` entry for audits, or on another runner's when
the audit's runner holds the most assignments under the plan; `dispatch` chooses.

### 4. Wait for a job to end

    docket jobs --wait --timeout 300

It returns when a running job ends, or after five minutes so you renew the
lead; run it in the foreground or in the background, and when it returns go
on. Then, for each job that ended since you last looked, by its report:

**DONE, for a ticket.** Hand it to the gate queue at once; dispatching goes on
meanwhile:

    docket land T14

`land` collects the job's change (the commit on the job's branch with the
message the job proposed, its `OBSERVE` lines added to the item), queues the
build, and starts the one background gate run for the checkout if none is
going. That run merges the queued builds onto the head of the work branch in a
worktree, runs the `gate` fact's command, fast-forwards the branch and closes
each item with its sha; a build that finishes while it gates goes into its next
batch. `docket land` with no id queues every finished build. A collect that
cannot commit keeps the job, so fix what it names and land again. A report of
DONE with no commit on the job's branch is a lost change, sent back as FAILED.

The lead never runs a gate itself and never merges a build by hand: no
worktree for a batch, no fast-forward, no background command of its own.
`docket jobs` lists the gate run beside the jobs, with the builds of its
batch, and `docket jobs --wait` returns when it exits.

- **A conflict `land` names** is the lead's to resolve, there and then. Merge
  the work branch into the job's branch in a worktree, read both sides and the
  items behind them, keep what each change meant, commit, and queue it again
  with `docket land T14`:

      git worktree add ../merge-T14 <the job's branch>
      git -C ../merge-T14 merge --no-ff -m "Merge <the job's commit subject>" <your branch>
      git worktree remove ../merge-T14

  Never leave a finished build waiting for a job to
  rebase it.
- **A failed gate.** Read it with `docket land --log`. Where the failure is
  the batch's merge or something mechanical (formatting, a generated file), fix
  it on the job's branch and queue it again. Where the batch split finds a
  build's own behaviour failing, send that build back with the failing lines
  and dispatch it again at once; the rest of its batch is queued again by
  `land`:

      docket land --back T14 "fails <gate> on <sha>: <the failing lines>"
      docket dispatch T14

`docket stale` lists the job branches whose item is closed or that no claim or job names, and
whether each holds commits the work ref lacks; `docket stale --prune` removes those with none.

Check content, not reachability: `git show HEAD:<a file it touched>`.

**DONE, for an audit.** Close the plan with the job's note:

    docket collect A3
    docket --branch <the job's branch> close A3 "<the NOTE line>"

**DONE, for a plan.** Merge any commit as for a ticket, then give the claim
back with the note. The plan stays open until its tickets close and its audit
is due: `close` refuses while it has open work.

    docket collect A3
    docket --branch <the job's branch> unclaim A3 --outcome ended "<the NOTE line>"

**DONE, for an investigation or a decided question.** Merge any commit as for a
ticket, then close it with the note, or for a decided question, with what it
opened: `docket --branch <branch> close Q7 "opened T22, T23"`.

**WAITING Q\<n\>.** The job filed a question and the item waits on it. The
wait already gave the claim back, so `unclaim` is refused: note the wait on the
item and clear the job:

    docket collect T14 --discard
    docket --branch <the job's branch> edit T14 --append "waits on Q<n>"

**A usage limit.** `docket collect` prints `usage limit: RUNNER on MACHINE until
RESET` and the server records it, so `docket machines` and the next dispatch
read it. Clear the job and give the claim back with the outcome `ended` and that reason. It is not
a failed job: it neither counts toward the second failure of the item nor
toward the stop below.

    docket collect T14 --discard
    docket --branch <branch> unclaim T14 --outcome ended "RUNNER usage limit until RESET"

**FAILED, or lost, or no report.** Read why (`docket jobs`, and the job's log
on its machine with `docket job log NAME`). Clear it with `docket collect T14
--discard`, give the claim back with the reason, and
dispatch it once more. The server counts the failed attempts: at the project's
`failure_limit` (2 by default) the unclaim assigns the item to the owner.

    docket --branch <branch> unclaim T14 --outcome failed "failed on <runner>: <why>"

When the last three ended attempts of the project all failed, `next` prints a
`Halt:` line: something is wrong with a machine or the setup, so dispatch
nothing more and stop after the running jobs.

### 5. Again

Back to step 1. Never end a turn without a watch armed: while a job or a gate
run is going, `docket jobs --wait --timeout 300` is running, and it returns on
every job that ends, on the gate run's exit and after five minutes. On each
return go round the loop again. A `Stall:` line from `next` is acted on in the
same pass. Work you do yourself (a measurement, a reference run) never waits for a quiet
machine or for the gate: start it on a machine the gate does not measure.

Stop when no job, no gate run and no work of your own is running, and step 3
dispatched nothing: every item left is blocked, parked on the owner or held by
another session.

## Rules

- Never build a ticket, review or answer anything yourself: a job does it.
  Landing is yours through `docket land`, including every conflict it names
  and the mechanical fixes a gate asks of a merge. Only a build whose own behaviour fails goes back to a job.
- Every claim is a running job or a build in the gate queue. A finished
  build is collected in the same pass and lands in the next gate run, or is
  given back with its reason; it is never held for another reason.
- Never push. Never `--force` a claim, and never unclaim one you did not make.
- Never rewrite the work ref: the squash skill squashes it in place, and only while nothing is
  claimed.
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

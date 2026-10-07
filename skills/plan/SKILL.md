---
name: plan
description: Turn a goal into a plan and its tickets in the docket of the project you are in, run the investigations, and turn decided questions and measured results into tickets. Takes a goal from the owner or a new plan, an investigation or a decided question from the queue. Every ticket names its evidence, its fix and the test that fails until it is fixed, and is linked to the plan it delivers. Use when asked to plan something, file tickets for a goal, work the investigations, or turn decisions into work.
---

# plan

Run `docket skills` first. It prints the facts of the project this directory
belongs to: the owner, where a real number comes from, and what only the owner
can do. This skill names the facts and carries none of them, so one copy
serves every project.

A plan is the one grouping: an `A` item stating a goal, with every ticket that
delivers it under it as its parent. When its last ticket closes, its audit
falls due on its own, once. This skill writes plans and tickets; it
never builds them.

## What to take

    docket status
    docket next 10 --role plan

The plan role takes three things, each claimed with `docket --branch
plan/a3-$RANDOM start A3` before it is worked:

- a plan with nothing under it yet: write its tickets;
- an investigation (`I`): measure it, then turn the result into tickets;
- a decided question the owner answered: turn the decision into tickets.

A goal the owner gives in conversation is a new plan, filed first.

## A plan

    docket new A "the goal, in one line" --body -

The body:

- **Source.** Where the goal came from: the conversation and its date, the
  owner's words quoted, or the item it grew from.
- **Principles.** Numbered claims the tree must satisfy when the plan is done,
  each checkable by reading the code or running a test. The audit checks
  these.
- **Scope.** What it touches and what it leaves alone.
- **Done when.** The state that closes it.

Before filing, `docket search` the goal's words: a plan or ticket that already
covers part of it is linked, not written again.

## Its tickets

One ticket per change that lands on its own, each in the shape the work skill
reads:

    docket new T "the change, in one line" --body -

- **Evidence.** What is wrong or missing now, with `file:line` read from the
  working tree, never quoted from a document.
- **Fix.** What the change does, and where its one owner lives.
- **Failing case.** The test that fails until the fix lands, at the lowest
  level that can fail for the right reason.

A defect is a `B` rather than a `T`.

Every item names its area, or files under its plan with `--parent`, beside its release:
`docket new T "the change" --parent A3 --release current`. Without either the write is refused,
and the refusal lists the project's areas. A plan names its area: `docket new A "the plan" --area NAME`.

Name every repository an item changes with `--repo PATH`, once for each: `docket new T "..." --repo PATH`.
Use `--repo @OWNER/NAME` for another project's root, and `docket edit ID --repo PATH` to change them
later. An item naming none takes its plan's repositories when it has a parent, else the `checkout`
fact; an item filed with no parent names its own.

Then:

    docket parent T14 T15 T16 A3        # an item has one parent, and it is a plan
    docket priority T14 high            # critical, high, normal or low; normal is the default
    docket rate T14 medium              # complexity: high, medium or low
    docket edit T14 --set group=name    # tickets one session should take together
    docket label add A3 slow-path --about "what the label means"   # a plan's labels are read by every item under it
    docket wait T15 --on T14            # T15 builds on T14, the owner ticket first

The queue reads release, then priority, then age. An item with no release is in
the backlog, which ranks after every release. Complexity is rated apart from
effort: high is architecture (a new data model, a migration, many dependants).

Unclaim the plan once its tickets are filed (`docket unclaim A3 --outcome ended`): it then
waits on them, and comes back for its audit when they are all closed.

## An investigation

Measure what the item asks, with what `docket skills measure` names as the
source of a real number. Write the measured result into its body (`docket edit
I7 --append "..."`), file the tickets it calls for, put them under the plan the
investigation serves, record that it spawned them, and close it with what it
opened:

    docket parent T20 T21 A3
    docket link T20 T21 origin I7
    docket close I7 "measured: <the number>; opened T20, T21"

The close's words are a record for a person; they link nothing.

A result that leaves a choice only the owner can make is a question, below.

## A decided question

Read the decision and the question's options, file the tickets that carry it
out, put them under the plan they serve, record that the question spawned them,
and close it with them:

    docket parent T20 T21 A3
    docket link T20 T21 origin Q5
    docket close Q5 "opened T20, T21"

## Questions

A routine choice of implementation is a line in a ticket's Fix, never a
question. A question, derived or not, is for a choice the owner will see in
the product. One a decided question or ordinary practice settles is recorded,
marked derived, with the ticket that carries it out, filed first; it closes on
that ticket and never waits to be planned again:

    docket new Q "the decision, in one line" --body -
    docket answer Q<n> "the choice" --derived "the basis" --carried-by T14

The owner reads `docket derived` as a digest and overturns any of them by
answering again. A choice nothing settles (product direction, money, legal or
store matters, the owner's own devices and accounts) is filed for the owner
with every option, the `file:line` evidence for each, what is ruled out and
the one fact that would settle it. A ticket that turns on it waits:
`docket wait T14 --on Q<n>`. Never end a reply by asking the owner to choose.

The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.

## When you are done

Say `stop`. The docket holds what was filed; no summary.

---
name: plan
description: Turn a goal into a plan and its tickets in the docket of the project you are in, run the investigations, and turn decided questions and measured results into tickets. Takes a goal from the owner or a new plan, an investigation or a decided question from the queue. Every ticket names its evidence, its fix and the test that fails until it is fixed, and is linked to the plan it delivers. Use when asked to plan something, file tickets for a goal, work the investigations, or turn decisions into work.
---

# plan

Every tool call resends this whole conversation, so keep what you read and
print small. Run `docket skills` first: it prints the project's facts. Read a
long file by section (`rg -n '^#{1,3} ' FILE`, then `sed -n 'A,Bp' FILE`).

A plan is an `A` item stating a goal; every ticket that delivers it is linked
as opened by it. Its audit falls due on its own when its last ticket closes.
This skill files plans and tickets and never builds them.

## What to take

`docket next 10 --role plan` lists plans that opened nothing yet,
investigations, and decided questions. Claim one with `docket --branch
plan/a3-$RANDOM start A3`. A goal the owner gives in conversation is filed as
a new plan first.

## A plan

`docket new A "the goal" --body -`, the body holding **Source** (the owner's
words and date, or the item it grew from), **Principles** (numbered, each
checkable by code or a test), **Scope** and **Done when**. `docket search` the
goal first and link what already covers part of it.

## Its tickets

`docket new T "the change" --body -` per change that lands on its own (`B` for
a defect), the body holding **Evidence** (`file:line` from the working tree),
**Fix**, and **Failing case** (the test that fails until it lands, at the
lowest level that can fail for the right reason). Then `docket link T14 T15
opened A3`, `docket priority T14 high` where it is more urgent than normal,
`docket rate T14 medium`, and `docket release A3`: the plan waits on its
tickets.

## An investigation

Measure it with the source `docket skills measure` names, append the result
(`docket edit I7 --append "..."`), file and link the tickets it calls for, and
`docket close I7 "measured: ...; opened T20, T21"`.

## A decided question

File the tickets that carry the decision out and `docket close Q5 "opened
T20, T21"`, which links them.

The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.

## Questions

A choice a decided question or ordinary practice settles: `docket new Q "..."
--body -`, then `docket answer Q<n> "the choice" --derived "the basis"`. One
nothing settles goes to the owner as a question with every option, its
`file:line` evidence, what is ruled out and the fact that would settle it; a
ticket that turns on it gets `docket wait T14 --on Q<n>`.

Say `stop` when done. No summary.

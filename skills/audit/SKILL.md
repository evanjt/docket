---
name: audit
description: Audit a finished plan in the docket of the project you are in, once, when every ticket it opened is closed. Checks each of the plan's principles and each ticket's failing case against the working tree with file:line, files every gap as a ticket under the plan, and closes the plan in one round. Never builds and never fixes. Use when asked to audit, check finished plans, or work the audits due.
---

# audit

Run `docket skills` first. It prints the facts of the project this directory
belongs to: the owner, where a real number comes from, and what only the owner
can do. This skill names the facts and carries none of them, so one copy
serves every project.

A plan's audit falls due on its own, once, when everything it opened is
closed. One round: what the audit finds missing becomes tickets under the
plan, and the plan closes. The tickets are worked like any other, and the plan
is not audited again. The audit reads and files; it never builds and never
fixes, not even a one-line gap.

## 1. Take a plan that is due

    docket next 10 --role audit
    n=$RANDOM
    docket --branch audit/a3-$n start A3

A refusal means another session holds it: take the next.

## 2. Read all of it

    docket show A3
    docket deps A3          # everything it opened, done and dropped alike
    docket audit A3         # its principles and the items that serve each

Read the plan's principles and every ticket it opened, in full. Then read the
tree as it stands on the branch the work landed on, not as the tickets
describe it.

## 3. Check it against the tree

- Each principle: does the tree satisfy it? Cite the `file:line` that shows
  it, or the test that asserts it and its result when run.
- Each closed ticket: does its failing case pass now? Run the test it names.
- Each dropped ticket: is what it described gone, or only moved? Read the
  code it cited.
- The fix class, not only the instances: grep the whole tree for the fact a
  principle names, so a neighbour the tickets did not list is found too.

Never quote a document as evidence. Re-read the working tree.

## 4. File each gap under the plan

    docket new T "the gap, in one line" --body -
    docket link T30 opened A3

The body holds the **Evidence** (`file:line`, or the failing test and its
output), the **Fix**, and the **Failing case**, as the plan skill writes them.
A defect is a `B`. Give each the priority it earns: a gap that breaks what the
plan promised is high or critical, a cosmetic one low. A finding outside the
plan's principles and area is filed the same way but linked as related, not
opened (`docket link T31 related A3`), and left for planning.

Every item filed names its release with `--release`, chosen by what it is,
in this order. Its nature: a crash, a hang, data loss or wrong numbers,
migration or upgrade safety, security or privacy, or release work is
`--release current` at high priority. What it serves: a blocker or member of
current-release work is `--release current`. Otherwise a feature or polish
goes to a later release, and tests, CI and hooks to the theme the project
keeps for them (`docket skills releases` lists the releases, the current
first). The body's last line says which and why: `**Release.** current: it
loses the draft on resume.`

A choice only the owner can make is a question with every option and its
evidence (`docket new Q ... --body -`); it does not hold the audit.

## 5. Close the plan

    docket close A3 "audited: principles 1-5 checked at <sha>, tickets' tests pass; gaps T30, T31"

Name what was checked, the commit it was checked at, and every gap filed, or
`clean` when there is none. The plan closes with its gaps open: they are its
remainder, worked as tickets.

The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.

## When you are done

Say `stop`. The docket holds the audit; no summary.

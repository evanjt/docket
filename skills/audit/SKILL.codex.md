---
name: audit
description: Audit a finished plan in the docket of the project you are in, once, when every ticket it opened is closed. Checks each of the plan's principles and each ticket's failing case against the working tree with file:line, files every gap as a ticket under the plan, and closes the plan in one round. Never builds and never fixes. Use when asked to audit, check finished plans, or work the audits due.
---

# audit

Every tool call resends this whole conversation, so keep what you read and
print small. Run `docket skills` first: it prints the project's facts. Read a
long file by section (`rg -n '^#{1,3} ' FILE`, then `sed -n 'A,Bp' FILE`).

A plan's audit falls due once, when everything it opened is closed. One
round: gaps become tickets under the plan and the plan closes. Never build or
fix, not even a one-line gap.

1. `docket next 10 --role audit`, then claim one:
   `docket --branch audit/a3-$RANDOM start A3`. A refusal means take the next.
2. `docket show A3`, `docket deps A3` and `docket audit A3`: the principles,
   and every item it opened, done and dropped.
3. Against the tree on the branch the work landed on: each principle holds,
   with the `file:line` or the test that shows it; each closed ticket's
   failing case passes when run; each dropped ticket's subject is gone, not
   moved. Grep the whole tree for the fact a principle names, not only the
   files the tickets cite.
4. Only a critical or high gap is filed, one that breaks what the plan
   promised: `docket new T "the gap" --body -` with **Evidence**, **Fix** and
   **Failing case** (`B` for a defect), and `docket link T30 opened A3`. A
   normal or low gap is an observation in the close note, never a ticket. A
   finding outside the plan's area is an observation too, unless critical or
   high: then it is linked `related` and left for planning. A choice only the owner can make is a
   question; it does not hold the audit.
   Every item filed names its release with `--release`, by what it is: a
   crash, hang, data loss, wrong numbers, upgrade safety, security, privacy or
   release work is `current` at high priority; a blocker or member of current
   work is `current`; otherwise a feature or polish goes to a later release, and
   tests, CI and hooks to the theme the project keeps for them. The body's last line says
   which and why (`**Release.** current: ...`).
5. `docket close A3 "audited: principles 1-5 checked at <sha>; gaps T30, T31;
   observed: ..."`, or `clean` with no gaps. The gaps stay open as tickets.

Say `stop` when done. No summary.
The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.


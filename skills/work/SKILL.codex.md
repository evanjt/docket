---
name: work
description: Work the docket of the project you are in, one ticket at a time, alongside other sessions on the same queue. Claims the most urgent ticket on a branch only this session knows, works it in a worktree off the branch the session started from with the failing test first, merges it back and closes it. Records a choice a prior decision settles, and files a question only the owner can answer and works another ticket while it waits. Use when asked to work the queue, take the next ticket, or run docket next.
---

# work

Every tool call resends this whole conversation, so what you read and print is
paid for again on every later step. Work one ticket at a time and keep output
small.

## Before anything

    docket skills

It prints this project's facts: owner, worktree and merge commands, traps,
where a real number comes from, what only the owner can do. That output and
`AGENTS.md`, already loaded, are the orientation you need. Read a long file by
section: list its headings with `rg -n '^#{1,3} ' FILE` and read one with
`sed -n 'A,Bp' FILE`. A refused docket command says why.

Run from the project root. Note the branch the session started on (`git branch
--show-current`): worktrees start from it and merges go back into it.

## The loop

1. `docket next 10 --role work`: tickets nobody holds, most urgent then oldest.
2. Claim on a branch nobody can guess:

       n=$RANDOM
       docket --branch audit/t14-$n start T14

   A refusal means another session holds it, it waits, or it is the owner's
   turn: take another. Never `--force` a live claim, never `release` one you
   did not make. A group on the row is one session's work: claim each member
   as you reach it. Then `docket similar T14`, so work landed under another id
   is not repeated.
3. `docket skills worktree` prints the worktree command; run it with your id
   and `$n`, from the branch the session started on. `docket skills traps`
   says what breaks in a fresh worktree. Write the failing test first, see it
   fail, then fix. Never `git stash`. Run the gates `AGENTS.md` names.
4. `docket skills merge` prints the merge; run it and read git's exit code.
   If another session merged first, rebase and merge again. Check content:
   `git show HEAD:<file> | grep <symbol>`. Never push. Then
   `docket close T14 <sha>`. A plan it names as released is due for an audit,
   another session's work.
5. Repeat until `docket next --role work` offers nothing you can claim, or the
   number you were asked for is reached.

Close a ticket to its full fix. A part left undone is its own ticket under the
same plan before the close: `docket new T "..." --body -`, `docket link T22
opened A3`, `docket edit T14 --append "Closed short of its fix: ..."`.

## Choices

A choice a decided question or ordinary practice settles is recorded and the
work goes on:

    docket new Q "the decision" --body -
    docket answer Q<n> "the choice" --derived "the basis"

A choice nothing settles (product direction, money, legal, the owner's devices
and accounts) is a question for the owner: `docket new Q "..." --body -` with
every option, its `file:line` evidence, what is ruled out and the fact that
would settle it, then `docket wait T14 --on Q<n>` and take another ticket. A
measurement nobody has taken is `docket new I`. Work only the owner can do is
`docket ask T14 "what is needed"`.

A side finding is `docket add "title" --body -`, a low-priority ticket. Never
weaken a test to avoid a question. Cite `file:line` from the working tree, not
a document.

Say `stop` when done. No summary.

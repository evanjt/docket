---
name: work
description: Work the docket of the project you are in, one ticket at a time, alongside other sessions on the same queue. Claims the most urgent ticket on a branch only this session knows, works it in a worktree off the branch the session started from with the failing test first, merges it back and closes it. Records a choice a prior decision settles, and files a question only the owner can answer and works another ticket while it waits. Use when asked to work the queue, take the next ticket, or run docket next.
---

# work

Run `docket skills` first. It prints the facts of the project this directory
belongs to: the owner, the worktree and merge commands, what breaks in a fresh
worktree, where a real number comes from, and what only the owner can do.
`docket skills owner` prints one. This skill names the facts and carries none
of them, so one copy serves every project.

Work the project's tickets one at a time, alongside other sessions taking from
the same queue. The project's `AGENTS.md`, already loaded, carries its own
rules.

## The loop

Run from the project root, the directory `docket skills` names. `docket` works
out the project from the directory, and a worktree under it resolves to the
same project. Note the branch the session started on (`git branch
--show-current`): every worktree starts from it and every merge goes back
into it.

### 1. Read the queue

    docket status
    docket next 10 --role work

`next` lists the tickets nobody holds, most urgent first, then oldest. Every
write lands on the server at once and every session reads it there.

### 2. Claim, on a branch only you know the number of

The claim is atomic. A second `start` on a held ticket is refused and says who
holds it, so there is no race to read back. The random suffix is for git: two
worktrees cannot share a branch name, and a name nobody can guess never
collides.

    n=$RANDOM
    docket --branch audit/t14-$n start T14

`--branch` is a global flag and goes before the verb. From inside a worktree
already on the branch, `docket start T14` reads it from `HEAD`.

Read what `start` printed.

- The row with your branch on it: the ticket is yours, carry on.
- A refusal: another session holds it, it waits on something, or it is the
  owner's turn. Take another. Never pass `--force` to take a claim that is
  not abandoned, and never `release` a ticket you did not claim.

`start` names the other files a live claim touches beside yours, and the
other members of a ticket's group: a group is one session's work, each member
claimed as you reach it.

Before you touch the code, `docket similar T14`. It lists what was done near
this ticket by title, cited file and symbol, so a fix that landed under
another id is not written twice.

### 3. Work it in a worktree off the branch you started from

    docket skills worktree     # the command for this project; run it with your id and $n

The command names the project's main branch. Where the session started on
another branch, start the worktree from that branch instead. Read `docket
skills traps` before building anything: it says what breaks in a fresh
worktree here.

Write the failing test first, at the lowest level that can fail for the right
reason, and see it fail against the unfixed code. Then fix. Before fixing, name the one owner of the fact, or the full inventory
of it (every write site, caller and copy), and fix the listed items through it.
Never `git stash`;
use a patch file. Run the project's gates (its `AGENTS.md` names them) before
merging.

### 4. Merge back and close

    docket skills merge        # the merge command; run it and read git's exit code

Plain git, no wrapper, into the branch the session started from. Read git's
exit code, not a pipe's. If another session merged first, rebase on the new
head and merge again; never finish an interrupted merge. Then check content,
not reachability: `git show HEAD:<file the branch touched> | grep <symbol it
added>`. Never push: the owner pushes.

    docket close T14 <sha>

The printed row is the proof; a refused close prints why and changes nothing.
When the close was a plan's last open ticket, the plan is named as released:
it is due for its audit, which is another session's work.

Close a ticket to its full fix. A part that turns out wrong, blocked or out of
proportion is filed as its own ticket under the same plan, with the evidence
and the reason, before the close:

    docket new T "the part left undone, in one line" --body -
    docket link T22 opened A3
    docket edit T14 --append "Closed short of its fix: <the part> is T22 because <why>."

Remove the worktree once it is merged.

### 5. The next ticket

Repeat from step 2 until `docket next --role work` offers nothing you can
claim, or you reach the number of tickets you were asked for.

## Decide by a recorded decision or ordinary practice; ask only what neither settles

When a fix turns on a choice, look first for what settles it: a decided
question (`docket search`, `docket derived`, and the decided questions
`docket new Q` lists as close to a new one), or ordinary practice for this
kind of software. When one does, record it, carried by the ticket in hand, and
carry on:

    docket new Q "the decision, in one line" --body -        # options and evidence
    docket answer Q<n> "the choice" --derived "the basis" --carried-by T14

A routine choice of implementation is a line in the close note instead, never a
question: a question, derived or not, is for a choice the owner will see in the
product. Never pick the cheaper option because it is cheaper.

A question goes to the owner only when nothing settles it: product direction,
money, legal or store matters, the owner's own devices and accounts. The body
comes on stdin: every option, the `file:line` evidence for each, what is ruled
out, and the one fact that would settle it. A measurement nobody has taken is
an investigation instead:

    docket new Q "the decision, in one line" --body -
    docket new I "what has to be measured first" --body -
    docket wait T14 --on Q<n>

`wait --on` refuses an id that does not exist, so file first. The wait clears
itself when the question is answered. Then take another ticket. Never end a
reply by asking the owner to choose: the question is in the docket.

Work only the owner can do (`docket skills hands`), a login, a device, an
account, is handed over with `docket ask T14 "what is needed"`.

Every item filed names its release with `--release`, chosen by what it is,
in this order. Its nature: a crash, a hang, data loss or wrong numbers,
migration or upgrade safety, security or privacy, or release work is
`--release current` at high priority. What it serves: a blocker or member of
current-release work is `--release current`. Otherwise a feature or polish
goes to a later release, and tests, CI and hooks to the theme the project
keeps for them (`docket skills releases` lists the releases, the current
first). The body's last line says which and why: `**Release.** current: it
loses the draft on resume.`

Something found on the way that is not this ticket is filed only when it is
critical or high, or the order above puts it in the current release: `docket
new B "title" --body - --release current --priority high
--complexity medium`. Anything else is an
observation on the ticket, never a ticket of its own: `docket edit T14
--append "Observed: ..."`.

Never weaken a test to avoid a question. Never quote a document as evidence:
re-read the working tree and cite `file:line`.

## Under a lead

A lead may run the same queue: it claims each ticket itself and starts a job
for it with `docket job run`. A job follows the brief it was started with,
not this skill: it builds on the branch it was given and leaves its change
uncommitted for the lead to commit on its own machine, never claims, merges
or closes, and ends with a one-line report the lead reads.
`docket` refuses `start`, `release`, `close`, `drop` and `reopen` inside a
job. Beside a lead, take only what `docket next --role work` still offers;
the claims keep the two apart.

The repository may be public while the docket is private. Code, tests,
comments and commit messages never name a docket item, project, person or
machine: a comment states the rule itself, and tests use invented names.
`docket private check --staged` finds what slipped in, and the hooks
`docket private hook` installs run it on every commit and push.

## When you are done

Say `stop`. No report and no summary of what was worked: the docket holds it.

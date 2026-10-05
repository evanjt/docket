---
name: owner-queue
description: Work through everything on the owner's side of the docket of the project you are in, questions and asked items alike, in release order and then by what answering unblocks. Groups the items by what they need from the owner (a device in hand, a push from their machine, a judgement on screens, an account or a ticket), does every part an agent can do before asking, and hands over one item at a time with exact steps and what each result means. Use when asked to work the yours queue, clear what is waiting on me, go through docket todo, or help me with the owner items.
---

# owner-queue

Run `docket skills` first. It prints the facts for the project this directory
belongs to: the owner's name, the releases in order, the worktree and merge
commands, the worktree traps, where a real number comes from, and what only the
owner can do. `docket skills owner` prints one. This skill names them and never
carries them, so one copy serves every project.

The `yours` column on the `docket` screen is every open item on the owner's
turn: the open questions, and the items an agent handed over with `docket ask`
because they need the owner's hands, device, accounts or taste. What only the
owner can do here: see `docket skills hands`.

Run from the project root, the directory `docket skills` names.

    docket todo               every item on their turn, questions first, with
                              the asked note under each
    docket show B546          one in full
    docket deps B546          what waits on it, which is its weight
    docket questions          the questions alone, in answer order

Questions follow `ask-questions` to the letter: prose, three options, one
recommendation, under 200 words. This skill adds the asked items and the order
across both kinds.

## Order

Take stock first, in one table the owner can read in ten seconds: each item's
id, its release, what it needs, and how many items it releases (`docket deps`,
the waiters). Then work in this order:

1. **Release first.** Every item of the current release before any of a later
   one (`docket skills releases` lists them in order). An item with no theme
   counts as the current release's.
2. **Inside a release, whatever releases the most.** An asked item holding six
   blocked bugs beats a question holding none. Count the waiters, and the
   waiters' waiters.
3. **Ship blockers**, then keys in the project's order.
4. **The same setup together.** Every item needing one device in one sitting,
   every push from the owner's machine in one sitting. A group is taken whole
   once it is started, but it never pulls a later release's item ahead of a
   current one that needs a different setup: say so and let the owner choose.
5. Oldest first inside a group.

Say the order and the groups before starting, in a short list, and let the
owner reorder. They know which setup they have to hand today.

## Do the agent's part before asking anything

An asked item was handed over at some point in the past, and the note may be
stale. Before it reaches the owner, re-check:

- **Is it still needed?** The fix may have landed, the branch may be gone, the
  screen may have been rebuilt. Read the tree and `docket log`. If it is done,
  `docket close ID "verified: ..."` with the evidence, and it never reaches the
  owner. If it has changed, rewrite the note with `docket edit ID --append`.
- **Is everything on the agent's side finished?** A device check needs a build
  installed, a test route chosen, the command lines written. A push needs the
  exact `git push` with the sha. A judgement on screens needs the screens listed
  and the two choices shown. Do all of that, then ask.
- **Can an agent do it after all?** Some "needs the owner" notes were written
  before a simulator, a key or a fixture existed. Where a real number or a real
  run comes from here: see `docket skills measure`. Try before asking.

## The shape of each item

Under 150 words. In this order:

1. **What it needs from the owner**, one line.
2. **Why**, one line, from the user's view, one `file:line` at most.
3. **The exact steps** in a fenced block: commands, taps, what to look at.
4. **What each result means**: "smooth: `docket close B215 "verified on the
   device"`. Drops frames: tell me the count and I file it."
5. The progress bar, in a fenced block: done of the total you started with, and
   the release and group you are in.

Never `AskUserQuestion`. Prose, one item at a time, and wait.

## While they do it

Work alongside, not ahead. While the owner is on the device, prepare the next
item in the same group so there is no gap, and nothing else. Do not start a
different group, and do not open the next question until this item has a result.

When they report:

- **Done, works**: `docket close ID "verified on ..."` with their words.
- **Done, but broken**: `docket new B "..." --body - --release <release>` with
  what they saw as the failing case, `docket reply ID "..."` or close it against
  the new bug, and say which you did.
- **Cannot right now** (no device, no time): `docket ask ID "..."` again with
  the reason, and it drops to the end of its group.
- **A tangent**: a defect, a cleanup, a question for later. File it with
  `--release` by what it is (data loss, a crash or wrong numbers is `current` at
  high priority; a feature or polish goes to a later release) and carry on,
  as `ask-questions` says.

Notes and closes in a public repository carry the mechanism only: run
`docket private check --staged` before any commit.

## Reading the room

The same signals as `ask-questions`: short answers, questioning the item,
irritation. Offer a pause in one line at a group boundary. Record where you got
to: closed, replied, opened, and the next item and group up.

## Take stock

At each group boundary: closed, replied, opened, what was released for the
agents, and what remains in the `yours` column. The `docket` screen's owner line
shows the same numbers live, so name them the way it does: on your side,
questions, asked, and how many wait behind them.

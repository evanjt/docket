---
name: squash
description: Write the published commit messages for the project you are in and apply them with docket squash. Takes the groups docket squash proposes, writes one terse line per group in the project's commit style, applies them, and names the push command without running it. Use when asked to squash, publish the history, or write the published messages.
---

# squash

Run `docket skills` first. It prints the facts for the project this directory
belongs to, including the commit style its messages follow. Run from the project
root.

The work ref keeps one commit per landed item and is never rewritten and never
pushed. `docket squash` builds the published ref from it, grouped where plans
finish, and needs one message per group.

1. `docket squash` prints each proposed group, oldest first: the plans it
   finishes, the plans it holds in part, and the items it carries with their
   titles. `docket squash --cap N` joins groups until there are at most N.
2. Write one line per group into a file, oldest first, one line each and no
   blank lines between. A line is the project's commit style: one line, imperative,
   the first word capitalized, no trailing punctuation, no articles, actions
   joined with commas (`Add squash command, guard messages`). Describe what the
   code does. When a group finishes several plans, join them into one line.
   A line never carries an item id, a plan id, a branch name, a round or phase
   label, or the name of any tool or assistant.
3. `docket squash --messages FILE` applies them. It refuses when the line count
   differs from the group count, or when `docket private check` finds a name in
   a message or a diff. Fix the line or the diff it names and run it again.
4. Never push. End by naming the push command for the owner to run, from the
   published ref and remote the project's `publish` fact names.

The repository may be public while the docket is private. A message names no
docket item, project, person or machine. `docket private check --staged` finds
what slipped in.

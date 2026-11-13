---
name: squash
description: Publish the work branch of the project you are in as grouped commits on the published ref, before the owner pushes it. Runs docket squash to see the proposed groups, writes one terse line per group in the project's commit style, checks them for private names, writes them with docket squash --messages, and names the push command without running it. Use when asked to squash, publish the history, or write the published messages, and when docket next or docket published offers a squash.
---

# squash

Run `docket skills` first. It prints the facts for the project this directory
belongs to: `work` is the branch the landings are read from, `publish` names the
local published ref and the remote ref it is pushed to (such as `published
origin/main`). Run from the project root.

Each landed item leaves a merge on the work branch. A squash builds the
published ref from those landings, one commit per group, above what the remote
ref already holds. It never touches the work ref, so it may run while items are
claimed, and it never pushes.

## 1. See the groups

    docket squash

It prints each proposed commit: the landings it covers, oldest first, and the
items they closed. `--cap N` merges adjacent proposals until at most N remain.
When it prints nothing to publish, stop. When it refuses, the reason is in its
line (a published ref that is checked out, a remote ref the work branch does not
hold): report it to the owner and stop.

## 2. Write one line per group

One line per proposed commit, oldest first, in the project's commit style, the
rules of the job's commit line: imperative, the first word capitalised, at most
72 characters, no articles, no trailing punctuation, no prefix such as `feat:`,
compound actions joined with commas (`Add squash command, guard messages`).
Describe what the code does. When a group finishes several plans, join them into
one line. A line never carries an item id, a plan id, a branch name, a round or
phase label, or the name of any tool or assistant. Write the lines to a file
outside the repository.

## 3. Check them

    docket private check

Run it over the tracked files, and read each line against the same rule. A line
that names an item, project, person or machine is rewritten before it is used.

## 4. Write the published commits

    docket squash --messages FILE

It builds the commits on the published ref from the lines, one per proposal, and
refuses a line count that does not match. Run `docket squash` again: it offers
nothing more to publish.

## 5. Never push

End with the push the push ask carries, a fast-forward of the remote ref, for
the owner to run:

    git push origin published:main

where `published` and `origin/main` are the two words of the `publish` fact.
Never push it yourself, and never move the published ref or the work ref by hand.

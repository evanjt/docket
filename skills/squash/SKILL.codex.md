---
name: squash
description: Squash the work branch of the project you are in, in place, before the owner pushes it. Groups the landings since the remote ref, writes one terse line per group in the project's commit style, rewrites the work branch onto those commits, keeps the old history on a dated branch, moves the done items' shas onto the new commits, and names the push command without running it. Use when asked to squash, publish the history, or write the published messages.
---

# squash

Run `docket skills` first. It prints the facts for the project this directory
belongs to: `work` is the branch to squash, `publish` names the remote ref it is
pushed to (the last word, such as `origin/main`). The commands below write
`main` and `origin/main` for those two. Run from the project root, on the work
branch.

Each landed item leaves a merge on the work branch. A squash rewrites that
branch in place, onto one commit per group, above what the remote ref already
holds. The old tip stays on a dated branch, so every old sha stays reachable.

## 1. Check it is safe

    git status --short                        # prints nothing
    docket wip                                # nothing claimed
    git fetch origin
    git merge-base --is-ancestor origin/main main && echo holds

Stop on a dirty tree or a claimed item: a claim's branch holds the history the
squash replaces, so it would not merge back. Stop when the work branch does not
hold the remote ref: it has diverged, and that is the owner's to settle.

## 2. Group the landings

    git log --first-parent --reverse --format='%H %cI %s' origin/main..main

Each line is one landing. Cut a group where a plan finishes: the landing that
closes a plan's last ticket ends a group (`docket done` and `docket show` say
which items closed and under which plan). With few landings, one group is fine.
Every landing goes in exactly one group, in order.

If a landing in the range moves a submodule's pin (`git diff --stat origin/main
main` lists the path with no line counts), stop and tell the owner: a submodule
is squashed first, by its own pass.

## 3. Write one line per group

One line, imperative, the first word capitalised, no trailing punctuation, no
articles, actions joined with commas (`Add squash command, guard messages`).
Describe what the code does. When a group finishes several plans, join them into
one line. A line never carries an item id, a plan id, a branch name, a round or
phase label, or the name of any tool or assistant. Run `docket private check`
over the tracked files, and read each line against the same rule.

## 4. Rewrite the work branch

Keep the old tip, then build each group's commit on the tree and date of its
last landing, parented on the one before:

    old=$(git rev-parse main)
    kept=work-$(date +%F)                     # add -2, -3 when it exists
    git branch "$kept" "$old"
    parent=$(git rev-parse origin/main)
    # for each group, oldest first, with END its last landing and LINE its message
    date=$(git log -1 --format=%cI END)
    parent=$(GIT_AUTHOR_DATE=$date GIT_COMMITTER_DATE=$date \
      git commit-tree END^{tree} -p "$parent" -m "LINE")

The last commit's tree must be the old tip's. Then move the branch, naming the
old tip so a branch that moved meanwhile is refused:

    test "$(git rev-parse "$parent^{tree}")" = "$(git rev-parse "$old^{tree}")"
    git update-ref refs/heads/main "$parent" "$old"
    git status --short                        # still prints nothing

## 5. Move the done items onto the new commits

Every old commit a group squashed maps to that group's new commit. For each
group, `git rev-list PREV..END`, where PREV is the previous group's END (the
remote ref for the first), gives its old commits. Write `old new` per line, full
shas, to a file outside the repository, then:

    docket admin remap --dry-run MAPFILE
    docket admin remap MAPFILE

Check one: `docket show` on an item closed in the range names a commit now on
the work branch.

## 6. Never push

End with the push for the owner to run, a fast-forward of the remote ref:

    git push origin main

Name the dated branch the old history is kept on.

The repository may be public while the docket is private. A message names no
docket item, project, person or machine. `docket private check --staged` finds
what slipped in.

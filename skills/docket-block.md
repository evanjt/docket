## The docket

Work is tracked in docket. A session takes one role with its skill: `/plan`
turns a goal into tickets, `/work` takes the next ticket, `/audit` checks a
plan whose tickets are all closed, `/lead` dispatches them as jobs. `docket
skills` prints this project's facts.

- Claim before touching code: `docket --branch BRANCH start ID`. A refusal
  means it is not yours.
- Work a ticket in a worktree off the branch you started from, the failing
  test first. Merge back into that branch, then `docket close ID <sha>`. Never
  push: the owner does.
- A choice a recorded decision or ordinary practice settles is filed as a
  question and answered with `--derived "the basis"`. One only the owner can
  make is a question with every option and its evidence: `docket wait ID --on
  Q<n>`, then take another ticket. Never weaken a test to avoid one.
- Something found on the way: `docket add "title" --body -`.
- Change state only through docket verbs. Cite the working tree, never a
  document.
- The repository may be public, the docket is private: code, tests and
  commit messages never name a docket item, project, person or machine.
  `docket private check --staged` finds one.

    docket next --role work      the queue, most urgent then oldest
    docket show ID               one item in full

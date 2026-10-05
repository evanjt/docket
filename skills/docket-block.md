## The docket

Work is tracked in docket, one role a session: `/plan` turns a goal into
tickets, `/work` takes the next ticket, `/audit` checks a plan whose tickets
are all closed, `/lead` dispatches them as jobs. `docket skills` prints this
project's facts.

- Claim first: `docket --branch BRANCH start ID`.
- Work in a worktree off your starting branch, the failing test first.
  Merge back, `docket close ID <sha>`. Never push.
- A routine choice is a line in the close note. One the owner sees, settled
  by a decision or practice, is a question answered `--derived "the basis"
  --carried-by ID`. One only the owner can make is a question with every
  option and its evidence: `docket wait ID --on Q<n>`. Never weaken a test.
- Every item filed names its release (`--release`) and its area (`--area NAME`,
  or `--parent PLAN`, taking the plan's). Crash, data loss, security, upgrade
  safety: current, high. Else the plan's.
  A ticket under a plan takes its release, or earlier if critical or high.
- A side finding is filed only when critical, high or current; else noted.
- Change state only through docket verbs. Cite the tree.
- The docket is private: code, tests and commits never name an item,
  project, person or machine; `docket private check --staged` finds one.

    docket next --role work      the queue, in release order
    docket show ID               one item in full

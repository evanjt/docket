## The docket

Work is tracked in docket, one role per session: `/plan` turns a goal into
tickets, `/work` takes the next ticket, `/audit` checks a plan whose tickets
are all closed, `/lead` dispatches them as jobs. `docket skills` prints this
project's facts.

- Claim before coding: `docket --branch BRANCH start ID`.
- Work a ticket in a worktree off the branch you started from, the failing
  test first. Merge back, then `docket close ID <sha>`. Never push.
- A routine choice is a line in the close note. One the owner sees in the
  product, settled by a decision or ordinary practice, is a question answered
  `--derived "the basis" --carried-by ID`. One only the owner can make is a
  question with every option and its evidence: `docket wait ID --on Q<n>`.
  Never weaken a test to avoid one.
- Every item filed names its release (`--release`): current for a crash,
  data loss, wrong numbers, security or what current work needs, else later.
- A side finding is filed only when critical, high or current-release, else
  noted on the ticket.
- Change state only through docket verbs. Cite the working tree.
- The repository may be public, the docket is private: code, tests and
  commits never name an item, project, person or machine.
  `docket private check --staged` finds one.

    docket next --role work      the queue, most urgent first
    docket show ID               one item in full

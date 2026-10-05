---
name: ask-questions
description: Go through the open Q questions in the docket of the project you are in, with the owner, and record the decisions. One question at a time in prose, ordered by release and then by what answering unblocks, so the owner is not context switching. Every question gets at least three options, one marked as the recommendation with the reasoning behind it, a worked example, and a code snippet where that is what makes it concrete. Use when asked to go through the questions, do questiontime, or answer the open Qs.
---

# ask-questions

Run `docket skills` first. It prints the facts for the project this directory
belongs to: the owner's name, the releases in order, where a real number comes
from, and what only the owner can do. `docket skills owner` prints one. This
skill names them and never carries them, so one copy serves every project.

Working the open questions with the owner is a conversation, not an
interrogation. Run from the project root, the directory `docket skills` names.

    docket questions          every open question, in answer order
    docket show Q116          one question in full
    docket deps Q116          everything waiting on it, and what it waits on

## What to ask first

**Clear what a basis settles before the sitting.** A question that a prior
decision, a central idea or ordinary practice settles is answered with
`docket answer Q<n> "choice" --derived "basis"` first, not asked. `docket
questions` flags each one close to a decided question. Open the sitting with
`docket derived`, so the owner sees what was decided for them and can overturn
any of it. A routine choice of implementation is a note on the item, never a
question; a derived answer made for a ticket carries it with `--carried-by`.

**Take them in the order `docket questions` prints them.** The order is by
release first (`docket skills releases` lists them, the current first), then by
what answering unblocks, then the order to ask them in. The first question
listed is the one to ask first. Do not re-rank from the `holds` lines: a
question that has moved is a question somebody re-weighed on purpose.

- **Finish a release before the next.** Questions of the current release are
  asked before any later release's, and questions of one theme together inside
  a release. Swapping subject costs the owner more than it costs you.
- **A question with no theme counts as the current release's.** Read
  `docket deps` for what it holds. If it belongs to a later release, say so and
  ask it with that release, not before. Never move a question by editing its
  theme from here; a question's release is the planner's call.
- **A question already narrowed by a prior decision says so in a dated note.**
  Ask only the part that note leaves open, and name what it settled.
- **Skip a question that is waiting on an `I`.** It waits on a measurement, not
  on the owner. `docket waiting` lists them under what they wait on.
- A question whose answer is waiting on a measurement is not ready. Say so, open
  an `I` for the measurement with `--release`, `docket wait Q<n> --on I<m>` and
  move on.

Give a progress bar on every question, in a fenced block: the bar, `n/total`,
the percentage, and the release and question of the total. Count against the
open total you started with, not the live count.

## Keep it short, and say what it means for the user

- Under 200 words per question, including the options.
- Open with what a user of the product sees go wrong, in one or two sentences.
  The code mechanism comes after, in one sentence, with a single `file:line`.
- No walk through the call chain, no doc-comment quotes, no history of who
  narrowed what. That lives in the item; `docket show` has it.
- Options are one line each: what it does, what it costs.
- Recommendation is one line, then stop.

## The shape of each question

**Ask in prose. Never with an option-card tool such as `AskUserQuestion`.**
Option cards make it an interrogation; prose makes it a conversation the owner
can push back inside, and their pushback is where the value is.

Each question gets, in this order:

1. **Context.** What the code does now, cited `file:line` and read from the
   working tree, never quoted from a document.
2. **A worked example with real context.** The concrete failure, with real
   numbers from the owner's own data where they exist. Include a code snippet
   or a use case whenever the difference between the options is visible in
   code, in a screen, or in what a user would see.
3. **At least three options.** Say what each costs and what it rules out.
4. **One recommendation, named as your best guess, with the reasoning.** Lead
   with it. Say when it is an audit's own recommendation rather than yours, and
   say so when the audit admits it was wrong, or when the question was already
   asked and answered earlier.

The options are prompts, not a menu. The owner often answers off-menu and the
off-menu answer is often better than every listed one, so hold the
recommendation lightly.

**Flag tensions between the owner's own answers** rather than recording them
quietly.

## The part that actually makes it work

- **Measure instead of arguing.** A question that has circled on an estimate
  nobody checked deserves the number, not another round. Where a real number
  comes from here: see `docket skills measure`. Write the number into the item
  so it is never argued again.
- **Take challenges seriously and go and read the code.** A challenge to the
  framing is usually right about what the product is meant to do, and the code
  is usually doing something else.
- **When a question is genuinely not answerable, say so and open an `I`**, then
  `docket wait Q<n> --on I<m>`. That is a right outcome, not a failure to get an
  answer. Record any leaning the owner has inside the investigation.
- **Open work items for what the asking turns up**, none folded into an answer.

## Reading the room

Long sittings go sour before they say so. Watch for it and offer the exit
rather than pushing to the end of the release.

The signals, roughly in order of how early they show up:

- Answers get shorter: one word, "fine", "whatever you think" from someone who
  has been arguing every option.
- They stop engaging with the options and question the question: "why are we
  even asking this", "didn't I answer this already".
- Terseness turns into irritation: "just", "no", flat contradiction with no
  reasoning attached.
- They go quiet on a question they would normally have an opinion about, or
  accept your recommendation twice in a row unexamined.

When you see it, **offer a pause in one line and let them choose**. "Want to
stop here? We're four into the current release, the rest keep." No diagnosis,
no naming what you think they are feeling, no apology.

Two things it often means rather than tiredness, and both are yours to fix:

- **The question is badly framed.** Re-read the code, reframe it, and say that
  is what you are doing.
- **It was already answered.** Run `docket similar Q<n>` and
  `docket search <words> --key Q --state done` before asking anything that
  smells familiar. `docket ask` prints the closest done decisions for the same
  reason. Being asked twice is the complaint made most.

If you pause, record where you got to: answered, waiting, opened, measured, and
the next question up. Every answer is written at once, so the next sitting
starts where this one ended.

## Take stock

At a release boundary, or whenever asked: answered, waiting, opened, measured,
then two or three things worth flagging. Say plainly that every `docket answer`
leaves the question open with a decision, which is agent work queued in
`docket research`, not another decision waiting on the owner.

## Check the answer covers the question, before recording it

An answer often carries more than was asked: the reasoning behind the choice, a
constraint nobody had written down, a tangent. That is the most valuable part of
the sitting. It is also how the actual decision gets left unmade, because the
interesting part of the reply is not always the part the question needed.

Before `docket answer`, re-read the question and check each thing it asked has
landed:

- A question with two parts needs both. An answer about phrasing settles one
  decision of two when the question was phrasing and a count.
- A question that names a threshold, a default, a screen or a file needs that
  specific thing chosen, not the principle behind it.
- An answer of principle is not a decision if the options differ inside it.

If part of it is still open, **ask again for that part only**. Name what is
settled first so the owner hears it landed, then ask the one thing that is not,
in a sentence. Do not re-ask the whole question or re-list the original options.

**Rebuild the recommendation from what they just said.** The reply usually rules
options out or adds a constraint, so the recommendation that led the first ask
is often dead. Give a fresh one grounded in their reasoning, and say which part
of their answer it follows from.

Never close the gap by guessing the owner's leaning. A basis is different: a
prior decision found with `docket similar` or `docket search`, a central idea,
or ordinary practice that settles it, stated as derived so the owner can
contradict it.

Then handle the surplus, rather than burying it in the answer text. Every item
filed names its release with `--release`, by what it is: data loss, a crash,
a hang, wrong numbers, upgrade safety, security or privacy is `current` at high
priority; a blocker or member of current work is `current`; a feature or polish
goes to a later release (`docket skills releases` lists them):

- A tangent that is really a defect or a cleanup: `docket new B "..." --body -
  --release <release>` or `docket new D ...`.
- A claim neither of you can settle without a number: `docket new I "..."
  --body - --release <release>`, and say you are opening it.
- Something that answers or reframes a **different** open question: say so, and
  ask it next rather than recording the words against a question the owner was
  not looking at.

## Recording each answer

    docket answer Q116 "..."

Quote the owner's own words where they carry the intent. `answer` appends the
decision to the body, dated, and unblocks every item waiting on the question.
The question stays open, on the agents' turn, until a research agent turns the
decision into items and runs `docket close Q116 "opened ..."`.

**`answer` is the only state change this skill makes on a question.** Never
`docket start` it, never `docket new` the items it owes, never `docket close`
it. An answered question moves from `yours` to `decided`, where it stays until
an agent takes it, and that move is how the owner sees the answer was taken.

The one piece of work allowed before `answer` is on the question itself. If the
conversation turned up context the next agent will need, a file named, a
constraint, a number, put it on the question with
`docket edit Q116 --append "..."` first, so the decision is read against it.
Nothing else is opened, claimed or closed from this skill except the `I`, `B`
and `D` items the surplus list names.

In a public repository, answers and notes carry the mechanism only: run
`docket private check --staged` before any commit.

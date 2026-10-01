//! What every page, state word and key means.

use crate::doc::{Doc, seg};
use crate::style;

const HELP: &str = "What this screen shows

  docket is a work register: every project's tickets, questions, plans and packages, kept by one
  server. This screen reads it through the server and reads again whenever the server says the
  database moved, so what it shows is current. Every write below is one request to the server, as
  the command line makes it; a refusal shows at the foot, in the server's words, and nothing moves.

Pages

  Home        every project: the loop's mode, its open tickets, what waits on you (YOURS), its flow.
              The mode is red when the project lacks a fact the loop needs. Enter opens a project.
  Project     the head: the project, the release it works towards, the loop's mode (RUN, PAUSE or
              DRAIN), jobs running of the slots the pool allows, the pace of closes and how long the
              ready work takes at it. Under it, in red, why nothing is worked when the facts say so,
              naming each missing fact; S shows them.
              The flow and LISTS: every count and list is a hot spot that opens it in the browser.
              NEXT       the tickets a free slot takes next, in order, with their package
              JOBS       every claim: its branch, the machine, how long it has run
              PACKAGES   packages started: done of all their tickets, and how many are worked now
              MOVES      every open, close, claim and decision, a row per minute, newest first
  Browser     a list beside the selected row in full: its facts, body, ties and log. Every id in it
              is a hot spot; opening one lists that item and everything tied to it.
  Yours (o)   what waits on you, then the open questions, one at a time with its whole body:
              a answers a question, r replies to a parked item, R retries what the loop parked.
  Plans (t)   plans, stories, packages, concepts and central ideas with their progress. Space opens
              one's children, and the side shows what comes next under the selected one.
  Settings (S) every fact of the project, its value and what it means; red where the loop needs a
              fact the project lacks. Tab or j/k picks a fact, Enter edits it in place, and an
              empty value unsets it. A value the server refuses is named under the fact.

State words, each in its colour

  ready      in the release, waiting for a free slot
  building   a job or a person is working on it now, or a package whose tickets are open
  checking   a package whose tickets are all done, under its review
  done       closed, with the commit that fixed it
  blocked    waiting on another item or a condition
  parked     waiting on you: something only you can do
  inbox      filed by an agent, not yet placed by the planner
  later      held out of this release
  dropped    closed without doing
  standing   a concept or central idea, open for good

  A verb in MOVES takes the colour of the word it leads to: claimed is the yellow of building,
  closed the dim of done, decided the cyan of checking.

Keys

  Tab / Shift-Tab        the next or previous id or count
  j / k, Down / Up       the next or previous row of a list; on a page with no list, as Tab
  Enter, l               open the one selected
  Esc, h, Left           back                         f, Right   forward again
  /                      search, the list following each key; Enter keeps it, Esc cancels
  Space                  open or close a row's children in the plans; in the other lists, as x
  PgUp / PgDn            scroll
  g home   o yours   t plans   S settings   ? this page   . read everything again   q quit

Writing

  x                      mark or unmark the row; a move acts on the marked rows, else on the row
  X                      clear the marks
  a                      answer the question; on one an agent decided, answering again overturns it
  r                      reply to a parked item: back to the agents with what happened
  R                      retry: a fresh start for what the loop parked or sent back
  p / d                  pull into the release / defer to later
  !                      priority: then c critical, h high, n normal, l low
  c                      complexity: then h high, m medium, l low
  L                      link: then related ID or opened ID
  F                      fold packages: then the package they go into
  :                      any verb of the command line, as close T3 abc1234 or priority high; an
                         id left out is the selected row's, ids left out are the marked rows
  Enter                  sends a typed line; Ctrl-E opens it in $EDITOR, and saving sends it;
                         Esc drops it

The same from a terminal

  docket next              the ready queue           docket todo         what is parked on you
  docket show B14          one item in full          docket derived      what agents decided, and why
  docket skills            the project's facts       docket pool run     start working
";

#[must_use]
pub fn doc() -> Doc {
    let mut d = Doc::default();
    for line in HELP.lines() {
        let heading = !line.is_empty() && !line.starts_with(' ');
        let tone = if heading {
            style::bold()
        } else {
            ratatui::style::Style::default()
        };
        let mut segs = Vec::new();
        let mut rest = line;
        for w in [
            "ready", "building", "checking", "done", "blocked", "parked", "inbox", "later",
            "dropped", "standing",
        ] {
            if let Some(tail) = line.strip_prefix(&format!("  {w} ")) {
                segs.push(seg("  ", tone));
                segs.push(seg(w, style::word(w)));
                rest = tail;
                segs.push(seg(" ", tone));
                break;
            }
        }
        segs.push(seg(rest, tone));
        d.line(segs);
    }
    d
}

//! The screen on a terminal: drawing, keys and the mouse, the change stream, and `$EDITOR` for a long
//! line.

use std::io::Write;
use std::process::{Command, ExitCode};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, MouseEvent, MouseEventKind,
};
use ratatui::DefaultTerminal;

use docket_client::{Api, Config};

use crate::app::{App, Reloaded};
use crate::doc::Target;
use crate::page::Page;
use crate::source::{Http, Source};
use crate::starter::{self, Start};
use crate::view;

/// What the change stream tells the screen.
enum Notice {
    Live,
    Changed(docket_client::Event),
    Down,
}

/// The least time between two reads of a page, so a burst of commits is read once.
const SETTLE: Duration = Duration::from_secs(1);

/// How often the screen looks for a project that has no lead.
const LEAD_LOOK: Duration = Duration::from_secs(5);

/// An open page to read again off the UI thread, and how many inputs the screen had taken.
struct Job {
    page: Page,
    inputs: u64,
}

/// A page read again for the `Job` that carried `inputs`.
struct Read {
    reloaded: Reloaded,
    inputs: u64,
}

/// Starts a lead on this machine for a project that has none.
pub type Launcher = dyn Fn(&Start) -> Result<String, String>;

/// The terminal taken: raw, on the alternate screen, and reporting the mouse.
fn init() -> DefaultTerminal {
    let terminal = ratatui::init();
    let _ = crossterm::execute!(std::io::stdout(), EnableMouseCapture);
    terminal
}

/// The terminal handed back as it was found.
fn restore() {
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
}

/// A panic gives the mouse back too; the hook `ratatui::init` sets restores the rest.
fn release_mouse_on_panic() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
        hook(info);
    }));
}

/// Follows the server's change stream for good, reconnecting after a pause when it drops.
fn follow(config: &Config, tx: &Sender<Notice>) {
    loop {
        if let Ok(stream) = Api::new(config).and_then(|api| api.changes()) {
            for event in stream {
                let notice = if event.name == "hello" {
                    Notice::Live
                } else {
                    Notice::Changed(event)
                };
                if tx.send(notice).is_err() {
                    return;
                }
            }
        }
        if tx.send(Notice::Down).is_err() {
            return;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// Every notice waiting: whether the stream is up, and whether anything moved since the last read.
fn drain<S: Source>(rx: &Receiver<Notice>, app: &mut App<S>, mut moved: bool) -> bool {
    while let Ok(n) = rx.try_recv() {
        match n {
            Notice::Live => {
                moved |= !app.live && app.seen_live;
                app.live = true;
                app.seen_live = true;
            }
            Notice::Changed(event) => moved |= event.concerns(app.page.slug()),
            Notice::Down => app.live = false,
        }
    }
    moved
}

/// Reads each page handed over, until the screen drops its end. The screen goes on taking input
/// meanwhile.
fn read_pages(api: Api, jobs: &Receiver<Job>, done: &Sender<Read>) {
    let mut scratch = App::unread(Http(api));
    while let Ok(Job { page, inputs }) = jobs.recv() {
        let reloaded = scratch.reload(page);
        if done.send(Read { reloaded, inputs }).is_err() {
            return;
        }
    }
}

/// The text after `$VISUAL` or `$EDITOR` (else `vi`) edited it in a file of its own, the screen
/// handed back to the terminal meanwhile; `None` when the editor failed.
fn edit(terminal: &mut DefaultTerminal, seed: &str) -> Option<String> {
    restore();
    let path = std::env::temp_dir().join(format!("docket-{}.md", std::process::id()));
    let text = std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(seed.as_bytes()))
        .ok()
        .and_then(|()| {
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| "vi".into());
            let status = Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("docket")
                .arg(&path)
                .status()
                .ok()?;
            status
                .success()
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()
        });
    let _ = std::fs::remove_file(&path);
    *terminal = init();
    text
}

/// Where the screen's input comes from, so one step of the loop can be run on a script of events.
trait Events {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool>;
    fn read(&mut self) -> std::io::Result<Event>;
}

struct Terminal;

impl Events for Terminal {
    fn poll(&mut self, wait: Duration) -> std::io::Result<bool> {
        event::poll(wait)
    }

    fn read(&mut self) -> std::io::Result<Event> {
        event::read()
    }
}

/// Every event waiting, after at most `wait` for the first. Pointer motion is kept to its last place,
/// since the earlier ones only drew the pointer along the way. Whether the screen needs drawing again.
fn step<S: Source>(
    app: &mut App<S>,
    events: &mut impl Events,
    wait: Duration,
) -> std::io::Result<bool> {
    let (mut redraw, mut hover, mut wait) = (false, None, wait);
    let seen = app.map.layout;
    while events.poll(wait)? {
        wait = Duration::ZERO;
        match events.read()? {
            Event::Mouse(m) if m.kind == MouseEventKind::Moved => hover = Some(m),
            other => {
                settle_hover(app, hover.take());
                redraw = true;
                match other {
                    Event::Key(k) => app.key(k),
                    Event::Mouse(m) => app.mouse_stamped(m, seen),
                    _ => {}
                }
                app.map.invalidate();
            }
        }
    }
    Ok(redraw | settle_hover(app, hover))
}

/// Applies a pointer's last place, and whether it changed the selection or what is hovered.
fn settle_hover<S: Source>(app: &mut App<S>, hover: Option<MouseEvent>) -> bool {
    let Some(m) = hover else {
        return false;
    };
    let (cursor, hovered) = (app.cursor().clone(), app.hover.clone());
    app.mouse(m);
    *app.cursor() != cursor || app.hover != hovered
}

/// One look at the projects bound here, starting a lead where the rule allows. The screen is drawn
/// again when a start was tried.
fn look_for_leads(app: &mut App<Http>, launch: &Launcher) -> bool {
    let roots = docket_client::roots::path().map(docket_client::roots::Roots::load);
    let bound = |slug: &str| roots.as_ref().map(|r| r.of(slug)).unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let before = app.starts.clone();
    starter::tick(&app.source, &bound, &mut app.starts, now, launch);
    app.starts != before
}

fn run_loop(
    app: &mut App<Http>,
    rx: &Receiver<Notice>,
    reads: (&Sender<Job>, &Receiver<Read>),
    launch: Option<&Launcher>,
) -> std::io::Result<()> {
    let (jobs, done) = reads;
    let mut reading = false;
    let mut terminal = init();
    let (mut moved, mut read) = (false, Instant::now());
    let mut looked = Instant::now()
        .checked_sub(LEAD_LOOK)
        .unwrap_or_else(Instant::now);
    let mut redraw = true;
    while !app.quit {
        if redraw {
            terminal.draw(|f| view::draw(app, f))?;
        }
        redraw = step(app, &mut Terminal, Duration::from_millis(100))?;
        if let Some(seed) = app.editor.take() {
            let text = edit(&mut terminal, &seed);
            app.edited(text);
            redraw = true;
        }
        let live = app.live;
        moved = drain(rx, app, moved);
        redraw |= live != app.live;
        while let Ok(Read { reloaded, inputs }) = done.try_recv() {
            reading = false;
            if inputs == app.inputs {
                app.adopt(reloaded);
                redraw = true;
            } else {
                moved = true;
            }
        }
        if moved && !reading && read.elapsed() >= SETTLE {
            let job = Job {
                page: app.page.clone(),
                inputs: app.inputs,
            };
            if jobs.send(job).is_ok() {
                reading = true;
            } else {
                app.changed();
                redraw = true;
            }
            (moved, read) = (false, Instant::now());
        }
        if let Some(launch) = launch
            && looked.elapsed() >= LEAD_LOOK
        {
            looked = Instant::now();
            redraw |= look_for_leads(app, launch);
        }
    }
    Ok(())
}

/// The screen over the server the config names, opened on a project when one is given. With a
/// `launch`, it starts a lead for each project bound here that has none while it is open.
#[must_use]
pub fn main(
    config: Config,
    project: Option<String>,
    name: &str,
    launch: Option<&Launcher>,
) -> ExitCode {
    let api = match Api::new(&config) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{name}: {e}");
            return ExitCode::from(2);
        }
    };
    let reader_config = config.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || follow(&config, &tx));
    let (jobs, job_rx) = mpsc::channel();
    let (done_tx, done) = mpsc::channel();
    match Api::new(&reader_config) {
        Ok(reader) => {
            std::thread::spawn(move || read_pages(reader, &job_rx, &done_tx));
        }
        Err(e) => {
            eprintln!("{name}: {e}");
            return ExitCode::from(2);
        }
    }
    let mut app = App::new(Http(api));
    if let Some(slug) = project {
        app.open(Target::Project(slug));
    }
    release_mouse_on_panic();
    let result = run_loop(&mut app, &rx, (&jobs, &done), launch);
    restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{name}: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
#[path = "tests/run.rs"]
mod tests;

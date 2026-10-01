use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};

use docket_client::{Api, Config};
use docket_tui::app::App;
use docket_tui::doc::Target;
use docket_tui::source::Http;
use docket_tui::view;

/// What the change stream tells the screen.
enum Notice {
    Live,
    Changed,
    Down,
}

/// Follows the server's change stream for good, reconnecting after a pause when it drops.
fn follow(config: &Config, tx: &Sender<Notice>) {
    loop {
        if let Ok(stream) = Api::new(config).and_then(|api| api.changes()) {
            for name in stream {
                let notice = if name == "hello" {
                    Notice::Live
                } else {
                    Notice::Changed
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

/// The least time between two reads of a page, so a burst of commits is read once.
const SETTLE: Duration = Duration::from_secs(1);

/// Every notice waiting: whether the stream is up, and whether anything moved since the last read.
fn drain(rx: &Receiver<Notice>, app: &mut App<Http>, mut moved: bool) -> bool {
    while let Ok(n) = rx.try_recv() {
        match n {
            Notice::Live => {
                moved |= !app.live;
                app.live = true;
            }
            Notice::Changed => moved = true,
            Notice::Down => app.live = false,
        }
    }
    moved
}

/// `-p SLUG`, `--project SLUG` or `DOCKET_PROJECT`: the project to open on.
fn project_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "-p" || a == "--project" {
            return args.next();
        }
    }
    std::env::var("DOCKET_PROJECT")
        .ok()
        .filter(|p| !p.is_empty())
}

fn run(app: &mut App<Http>, rx: &Receiver<Notice>) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let (mut moved, mut read) = (false, Instant::now());
    while !app.quit {
        terminal.draw(|f| view::draw(app, f))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(k) = event::read()?
        {
            app.key(k);
        }
        moved = drain(rx, app, moved);
        if moved && read.elapsed() >= SETTLE {
            app.changed();
            (moved, read) = (false, Instant::now());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("docket-tui: {e}");
            return ExitCode::from(2);
        }
    };
    let api = match Api::new(&config) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("docket-tui: {e}");
            return ExitCode::from(2);
        }
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || follow(&config, &tx));
    let mut app = App::new(Http(api));
    if let Some(slug) = project_arg() {
        app.open(Target::Project(slug));
    }
    let result = run(&mut app, &rx);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("docket-tui: {e}");
            ExitCode::FAILURE
        }
    }
}

//! The screen on a terminal: drawing, keys, the change stream, and `$EDITOR` for a long line.

use std::io::Write;
use std::process::{Command, ExitCode};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::DefaultTerminal;

use docket_client::{Api, Config};

use crate::app::App;
use crate::doc::Target;
use crate::source::Http;
use crate::view;

/// What the change stream tells the screen.
enum Notice {
    Live,
    Changed,
    Down,
}

/// The least time between two reads of a page, so a burst of commits is read once.
const SETTLE: Duration = Duration::from_secs(1);

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

/// The text after `$VISUAL` or `$EDITOR` (else `vi`) edited it in a file of its own, the screen
/// handed back to the terminal meanwhile; `None` when the editor failed.
fn edit(terminal: &mut DefaultTerminal, seed: &str) -> Option<String> {
    ratatui::restore();
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
    *terminal = ratatui::init();
    text
}

fn run_loop(app: &mut App<Http>, rx: &Receiver<Notice>) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let (mut moved, mut read) = (false, Instant::now());
    while !app.quit {
        terminal.draw(|f| view::draw(app, f))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(k) = event::read()?
        {
            app.key(k);
        }
        if let Some(seed) = app.editor.take() {
            let text = edit(&mut terminal, &seed);
            app.edited(text);
        }
        moved = drain(rx, app, moved);
        if moved && read.elapsed() >= SETTLE {
            app.changed();
            (moved, read) = (false, Instant::now());
        }
    }
    Ok(())
}

/// The screen over the server the config names, opened on a project when one is given.
#[must_use]
pub fn main(config: Config, project: Option<String>, name: &str) -> ExitCode {
    let api = match Api::new(&config) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{name}: {e}");
            return ExitCode::from(2);
        }
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || follow(&config, &tx));
    let mut app = App::new(Http(api));
    if let Some(slug) = project {
        app.open(Target::Project(slug));
    }
    let result = run_loop(&mut app, &rx);
    ratatui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{name}: {e}");
            ExitCode::FAILURE
        }
    }
}

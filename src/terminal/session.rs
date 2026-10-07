//! One command running in a PTY, with alacritty_terminal keeping the screen.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use alacritty_terminal::event::{Event, EventListener, Notify, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::tty;
use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui_kit::*;

/// Lines of scrollback kept per command. A big `-Syu` fits comfortably.
const SCROLLBACK: usize = 10_000;

/// What to run in the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
}

impl Command {
    pub fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// The command as you'd type it, for the pane header.
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    /// The exit code, or `None` when the process was killed by a signal.
    Exited(Option<i32>),
}

/// Emitted once, when the command finishes.
pub struct Finished(pub Status);

/// Forwards terminal events from the PTY thread to the UI.
#[derive(Clone)]
pub struct Listener(UnboundedSender<Event>);

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        _ = self.0.unbounded_send(event);
    }
}

pub struct Session {
    pub command: Command,
    term: Arc<FairMutex<Term<Listener>>>,
    notifier: Notifier,
    status: Status,
    columns: u16,
    lines: u16,
    _events: Task<()>,
}

impl EventEmitter<Finished> for Session {}

impl Session {
    /// Starts `command` in a new PTY. Fails when the PTY or the program can't start.
    pub fn spawn(command: Command, columns: u16, lines: u16, cx: &mut App) -> std::io::Result<Entity<Self>> {
        let (tx, mut rx) = unbounded();
        let listener = Listener(tx);

        let size = TermSize::new(columns as usize, lines as usize);
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        let term = Arc::new(FairMutex::new(Term::new(config, &size, listener.clone())));

        // Set per child instead of with `tty::setup_env`, which would change
        // Packrat's own environment.
        let env = HashMap::from([
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("COLORTERM".to_string(), "truecolor".to_string()),
        ]);
        let options = tty::Options {
            shell: Some(tty::Shell::new(command.program.clone(), command.args.clone())),
            working_directory: std::env::var_os("HOME").map(Into::into),
            drain_on_exit: true,
            env,
        };
        let pty = tty::new(&options, window_size(columns, lines), 0)?;
        let event_loop = EventLoop::new(term.clone(), listener, pty, true, false)?;
        let notifier = Notifier(event_loop.channel());
        event_loop.spawn();

        Ok(cx.new(|cx| {
            let events = cx.spawn(async move |this, cx| {
                while let Some(event) = rx.next().await {
                    if this.update(cx, |session: &mut Session, cx| session.handle(event, cx)).is_err() {
                        break;
                    }
                }
            });
            Self {
                command,
                term,
                notifier,
                status: Status::Running,
                columns,
                lines,
                _events: events,
            }
        }))
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Wakeup | Event::MouseCursorDirty | Event::CursorBlinkingChange => cx.notify(),
            Event::PtyWrite(text) => self.write(text.into_bytes()),
            Event::ChildExit(status) => self.finish(Status::Exited(status.code()), cx),
            // The event loop also sends Exit after the child is gone; only the
            // first one counts.
            Event::Exit => self.finish(Status::Exited(None), cx),
            Event::ClipboardStore(_, text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Event::TextAreaSizeRequest(format) => {
                let reply = format(window_size(self.columns, self.lines));
                self.write(reply.into_bytes());
            }
            _ => {}
        }
    }

    fn finish(&mut self, status: Status, cx: &mut Context<Self>) {
        if self.status == Status::Running {
            self.status = status;
            cx.emit(Finished(status));
            cx.notify();
        }
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn term(&self) -> &Arc<FairMutex<Term<Listener>>> {
        &self.term
    }

    /// Sends input to the program. Ignored once it has exited.
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        if self.status == Status::Running {
            self.notifier.notify(bytes);
        }
    }

    pub fn resize(&mut self, columns: u16, lines: u16) {
        if (columns, lines) == (self.columns, self.lines) || columns == 0 || lines == 0 {
            return;
        }
        self.columns = columns;
        self.lines = lines;
        self.term.lock().resize(TermSize::new(columns as usize, lines as usize));
        _ = self.notifier.0.send(Msg::Resize(window_size(columns, lines)));
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Stops the PTY thread; the child gets SIGHUP when its terminal closes.
        _ = self.notifier.0.send(Msg::Shutdown);
    }
}

/// The terminal's size in cells.
struct TermSize {
    columns: usize,
    lines: usize,
}

impl TermSize {
    fn new(columns: usize, lines: usize) -> Self {
        Self { columns, lines }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

fn window_size(columns: u16, lines: u16) -> WindowSize {
    // Cell sizes only matter to programs that ask for pixel sizes.
    WindowSize {
        num_lines: lines,
        num_cols: columns,
        cell_width: 8,
        cell_height: 16,
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn displays_commands() {
        let command = Command::new("sudo", ["pacman", "-Rns", "firefox"]);
        assert_eq!(command.display(), "sudo pacman -Rns firefox");
    }
}

#[cfg(test)]
mod pty_tests {
    use std::time::Duration;

    use alacritty_terminal::grid::Dimensions as _;
    use alacritty_terminal::index::{Column, Line, Point};
    use gpui_kit::TestAppContext;

    use super::{Command, Session, Status};

    /// The text on one screen row.
    fn row(session: &Session, line: i32) -> String {
        let term = session.term().lock();
        let grid = term.grid();
        (0..term.columns())
            .map(|col| grid[Point::new(Line(line), Column(col))].c)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[gpui_kit::test]
    async fn runs_a_real_command_and_reports_its_exit_code(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let command = Command::new("sh", ["-c", "printf 'hello from packrat'; exit 3"]);
        let session = cx.update(|cx| Session::spawn(command, 80, 10, cx)).unwrap();

        for _ in 0..200 {
            if session.read_with(cx, |session, _| session.status()) != Status::Running {
                break;
            }
            cx.background_executor.timer(Duration::from_millis(10)).await;
            cx.run_until_parked();
        }

        session.read_with(cx, |session, _| {
            assert_eq!(session.status(), Status::Exited(Some(3)));
            assert_eq!(row(session, 0), "hello from packrat");
        });
    }
}

//! Draws a [`Session`] and handles typing, scrolling, selection and copy/paste.

use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Line, Point as GridPoint, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

mod screen;

use super::colors;
use super::keys;
use super::session::{Command, Finished, Session, Status};

const KEY_CONTEXT: &str = "Terminal";

// Keys the window or Packrat would otherwise take for themselves (Kit's root
// binds Tab to move focus). Bound in the terminal's own context, these win
// while the terminal has focus and go to the program instead.
actions!(terminal, [SendTab, SendBackTab, SendCtrlF, SendCtrlL, SendCtrlR]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", SendTab, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", SendBackTab, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-f", SendCtrlF, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-l", SendCtrlL, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-r", SendCtrlR, Some(KEY_CONTEXT)),
    ]);
}

const FONT_SIZE: f32 = 13.;
const LINE_HEIGHT: f32 = 18.;
const HEADER_HEIGHT: f32 = 36.;
const PADDING: f32 = 8.;

/// What the pane tells the window about.
pub enum PaneEvent {
    /// A command ended. `auto_close` is true for commands Packrat started
    /// (removals, updates), false for shells you opened yourself.
    Finished { status: Status, label: String, auto_close: bool },
    Hide,
}

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: Option<Entity<Session>>,
    /// The grid size that fits the current bounds.
    columns: u16,
    lines: u16,
    cell_width: Pixels,
    /// Top-left of the text area, in window coordinates, from the last paint.
    origin: Point<Pixels>,
    selecting: bool,
    /// Scroll that hasn't added up to a whole line yet.
    scroll_remainder: f32,
    _subscription: Option<Subscription>,
}

impl EventEmitter<PaneEvent> for TerminalView {}

impl TerminalView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            session: None,
            columns: 100,
            lines: 16,
            cell_width: px(8.),
            origin: Point::default(),
            selecting: false,
            scroll_remainder: 0.,
            _subscription: None,
        }
    }

    pub fn is_running(&self, cx: &App) -> bool {
        self.session.as_ref().is_some_and(|session| session.read(cx).status() == Status::Running)
    }

    /// Starts a command, replacing whatever ran before, and takes the keyboard
    /// so password and `[Y/n]` prompts can be answered right away.
    pub fn run(
        &mut self,
        command: Command,
        auto_close: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> std::io::Result<()> {
        let label = command.display();
        let session = Session::spawn(command, self.columns, self.lines, cx)?;
        self._subscription = Some(cx.subscribe(&session, move |_, _, Finished(status), cx| {
            cx.emit(PaneEvent::Finished {
                status: *status,
                label: label.clone(),
                auto_close,
            });
            cx.notify();
        }));
        self.session = Some(session);
        self.focus_handle.focus(window, cx);
        cx.notify();
        Ok(())
    }

    /// Runs your login shell (`$SHELL`, or bash) in the pane.
    fn open_shell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
        if let Err(err) = self.run(Command::new(shell.clone(), Vec::<String>::new()), false, window, cx) {
            window.push_notification(Notification::error(format!("Couldn't start {shell}: {err}")), cx);
        }
    }

    fn write(&self, bytes: Vec<u8>, cx: &App) {
        if let Some(session) = &self.session {
            session.read(cx).write(bytes);
        }
    }

    fn set_grid(&mut self, columns: u16, lines: u16, cx: &mut Context<Self>) {
        if (columns, lines) == (self.columns, self.lines) {
            return;
        }
        self.columns = columns;
        self.lines = lines;
        if let Some(session) = &self.session {
            session.update(cx, |session, _| session.resize(columns, lines));
        }
    }

    fn send_key(&mut self, bytes: &'static [u8], cx: &mut Context<Self>) {
        if self.is_running(cx) {
            self.write(bytes.to_vec(), cx);
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;
        let mods = &keystroke.modifiers;
        // Once the program has exited there's nothing to type into: Enter or
        // Esc closes the pane, the way a finished terminal tab usually does.
        if self.session.is_some() && !self.is_running(cx) {
            if matches!(keystroke.key.as_str(), "enter" | "escape") && !mods.modified() {
                cx.emit(PaneEvent::Hide);
                cx.stop_propagation();
            }
            return;
        }
        if mods.control && mods.shift && keystroke.key == "c" {
            self.copy(cx);
            cx.stop_propagation();
            return;
        }
        if mods.control && mods.shift && keystroke.key == "v" {
            self.paste(cx);
            cx.stop_propagation();
            return;
        }
        let Some(session) = &self.session else {
            return;
        };
        let app_cursor = session.read(cx).term().lock().mode().contains(TermMode::APP_CURSOR);
        if let Some(bytes) = keys::to_bytes(keystroke, app_cursor) {
            // Typing jumps back to the live screen and clears any selection.
            {
                let session = session.read(cx);
                let mut term = session.term().lock();
                term.scroll_display(Scroll::Bottom);
                term.selection = None;
            }
            self.write(bytes, cx);
            cx.stop_propagation();
        }
    }

    fn copy(&self, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let text = session.read(cx).term().lock().selection_to_string();
        if let Some(text) = text.filter(|text| !text.is_empty()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        let bracketed = session.read(cx).term().lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = if bracketed {
            format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
        } else {
            text.into_bytes()
        };
        self.write(bytes, cx);
    }

    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = &self.session else {
            return;
        };
        let delta = event.delta.pixel_delta(px(LINE_HEIGHT)).y / px(LINE_HEIGHT) + self.scroll_remainder;
        let whole = delta.trunc();
        self.scroll_remainder = delta - whole;
        if whole != 0. {
            session.read(cx).term().lock().scroll_display(Scroll::Delta(whole as i32));
            cx.notify();
        }
    }

    /// The grid cell under a window position, and which half of it.
    fn grid_point(&self, position: Point<Pixels>, display_offset: usize) -> (GridPoint, Side) {
        let x = (position.x - self.origin.x).max(px(0.));
        let y = (position.y - self.origin.y).max(px(0.));
        let column = ((x / self.cell_width) as usize).min(self.columns.saturating_sub(1) as usize);
        let row = ((y / px(LINE_HEIGHT)) as usize).min(self.lines.saturating_sub(1) as usize);
        let side = if (x / self.cell_width).fract() < 0.5 { Side::Left } else { Side::Right };
        let line = Line(row as i32 - display_offset as i32);
        (GridPoint::new(line, Column(column)), side)
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let Some(session) = &self.session else {
            return;
        };
        let session = session.read(cx);
        let mut term = session.term().lock();
        let (point, side) = self.grid_point(event.position, term.grid().display_offset());
        let kind = match event.click_count {
            2 => SelectionType::Semantic,
            3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        term.selection = Some(Selection::new(kind, point, side));
        drop(term);
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let Some(session) = &self.session else {
            return;
        };
        let session = session.read(cx);
        let mut term = session.term().lock();
        let (point, side) = self.grid_point(event.position, term.grid().display_offset());
        if let Some(selection) = term.selection.as_mut() {
            selection.update(point, side);
        }
        drop(term);
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.selecting = false;
        // A plain click leaves an empty selection behind; drop it.
        if let Some(session) = &self.session {
            let session = session.read(cx);
            let mut term = session.term().lock();
            if term.selection.as_ref().is_some_and(|selection| selection.is_empty()) {
                term.selection = None;
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (label, status) = match &self.session {
            Some(session) => {
                let session = session.read(cx);
                (session.command.display(), Some(session.status()))
            }
            None => ("Terminal".to_string(), None),
        };
        let (status_text, status_color) = match status {
            Some(Status::Running) => ("Running", theme.info_foreground),
            Some(Status::Exited(Some(0))) => ("Done", theme.success),
            Some(Status::Exited(Some(code))) => (if code == 130 { "Stopped" } else { "Failed" }, theme.danger_foreground),
            Some(Status::Exited(None)) => ("Stopped", theme.danger_foreground),
            None => ("", theme.muted_foreground),
        };
        let hint = matches!(status, Some(Status::Exited(_))).then_some("Enter to close");
        let exit_code = match status {
            Some(Status::Exited(Some(code))) if code != 0 => Some(format!("exit {code}")),
            _ => None,
        };
        let running = status == Some(Status::Running);

        h_flex()
            .h(px(HEADER_HEIGHT))
            .flex_shrink_0()
            .px_3()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(Icon::new(IconName::SquareTerminal).small().text_color(theme.muted_foreground))
            .child(div().flex_1().min_w_0().truncate().text_sm().font_family(theme.mono_font_family.clone()).child(label))
            .when(!status_text.is_empty(), |this| {
                this.child(
                    h_flex()
                        .flex_shrink_0()
                        .gap_1p5()
                        .items_center()
                        .text_xs()
                        .text_color(status_color)
                        .child(div().size(px(7.)).rounded_full().bg(status_color))
                        .child(status_text)
                        .children(exit_code.map(|code| div().text_color(theme.muted_foreground).child(code)))
                        .children(hint.map(|hint| div().text_color(theme.muted_foreground).child(hint))),
                )
            })
            .when(!running, |this| {
                this.child(
                    Button::new("terminal-new-shell")
                        .ghost()
                        .xsmall()
                        .label("New Shell")
                        .tooltip("Open your shell here")
                        .on_click(cx.listener(|this, _, window, cx| this.open_shell(window, cx))),
                )
            })
            .when(running, |this| {
                this.child(
                    Button::new("terminal-stop")
                        .ghost()
                        .xsmall()
                        .label("Stop")
                        .tooltip("Send Ctrl+C")
                        .on_click(cx.listener(|this, _, _, cx| this.write(vec![3], cx))),
                )
            })
            .child(
                Button::new("terminal-hide")
                    .ghost()
                    .xsmall()
                    .icon(IconName::ChevronDown)
                    .accessibility_label("Hide Terminal")
                    .tooltip("Hide Terminal")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PaneEvent::Hide))),
            )
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(colors::background())
            .child(self.render_header(cx))
            .child(
                div()
                    .id("terminal-screen")
                    .flex_1()
                    .min_h_0()
                    .p(px(PADDING))
                    .key_context(KEY_CONTEXT)
                    .track_focus(&self.focus_handle)
                    .cursor(CursorStyle::IBeam)
                    .on_key_down(cx.listener(Self::on_key_down))
                    .on_action(cx.listener(|this, _: &SendTab, _, cx| this.send_key(b"\t", cx)))
                    .on_action(cx.listener(|this, _: &SendBackTab, _, cx| this.send_key(b"\x1b[Z", cx)))
                    .on_action(cx.listener(|this, _: &SendCtrlF, _, cx| this.send_key(b"\x06", cx)))
                    .on_action(cx.listener(|this, _: &SendCtrlL, _, cx| this.send_key(b"\x0c", cx)))
                    .on_action(cx.listener(|this, _: &SendCtrlR, _, cx| this.send_key(b"\x12", cx)))
                    .on_scroll_wheel(cx.listener(Self::on_scroll))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .child(self.render_screen(window, cx)),
            )
    }
}

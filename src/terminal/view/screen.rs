//! Draws the terminal grid: one styled text line per row, plus the cursor.

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::super::colors;
use super::super::session::Status;
use super::{FONT_SIZE, LINE_HEIGHT, TerminalView};

impl TerminalView {
    pub(super) fn render_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let font = Font {
            family: cx.theme().mono_font_family.clone(),
            ..Font::default()
        };
        let font_id = window.text_system().resolve_font(&font);
        if let Ok(advance) = window.text_system().advance(font_id, px(FONT_SIZE), 'm') {
            self.cell_width = advance.width;
        }

        let Some(session) = self.session.clone() else {
            return v_flex()
                .size_full()
                .gap_3()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Commands you run from Packrat show up here")
                .child(
                    Button::new("open-shell")
                        .outline()
                        .small()
                        .label("Open Shell")
                        .on_click(cx.listener(|this, _, window, cx| this.open_shell(window, cx))),
                )
                .into_any_element();
        };
        let session = session.read(cx);
        let running = session.status() == Status::Running;
        let focused = self.focus_handle.is_focused(window);
        let term = session.term().lock();
        let content = term.renderable_content();
        let offset = content.display_offset as i32;
        let screen_lines = term.screen_lines();
        let colors_table = content.colors;

        let mut rows: Vec<(String, Vec<TextRun>)> = vec![(String::new(), Vec::new()); screen_lines];
        for indexed in content.display_iter {
            let cell = indexed.cell;
            if cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                continue;
            }
            let row = (indexed.point.line.0 + offset) as usize;
            let Some((text, runs)) = rows.get_mut(row) else {
                continue;
            };

            let (mut fg, mut bg) = (cell.fg, cell.bg);
            if cell.flags.contains(Flags::INVERSE) {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut fg = colors::resolve(fg, colors_table);
            if cell.flags.contains(Flags::DIM) {
                fg = fg.opacity(0.6);
            }
            let selected = content.selection.is_some_and(|range| range.contains(indexed.point));
            let background = if selected {
                Some(cx.theme().selection)
            } else if bg == Color::Named(NamedColor::Background) {
                None
            } else {
                Some(colors::resolve(bg, colors_table))
            };
            let ch = if cell.flags.contains(Flags::HIDDEN) { ' ' } else { cell.c };

            let mut run_font = font.clone();
            if cell.flags.contains(Flags::BOLD) {
                run_font.weight = FontWeight::BOLD;
            }
            if cell.flags.contains(Flags::ITALIC) {
                run_font.style = FontStyle::Italic;
            }
            let underline = cell.flags.intersects(Flags::ALL_UNDERLINES).then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: cell.flags.contains(Flags::UNDERCURL),
            });

            let start = text.len();
            text.push(ch);
            let len = text.len() - start;
            match runs.last_mut() {
                Some(last)
                    if last.color == fg
                        && last.background_color == background
                        && last.font == run_font
                        && last.underline == underline =>
                {
                    last.len += len;
                }
                _ => runs.push(TextRun {
                    len,
                    font: run_font,
                    color: fg,
                    background_color: background,
                    underline,
                    strikethrough: None,
                }),
            }
        }

        let cursor = content.cursor;
        let cursor_row = cursor.point.line.0 + offset;
        let show_cursor = running && cursor.shape != CursorShape::Hidden && (0..screen_lines as i32).contains(&cursor_row);
        let cursor_color = colors::resolve(Color::Named(NamedColor::Cursor), colors_table);
        let foreground = colors::resolve(Color::Named(NamedColor::Foreground), colors_table);
        drop(term);

        let cell_width = self.cell_width;
        let view = cx.entity().downgrade();
        let lines = rows.into_iter().enumerate().map(|(ix, (text, runs))| {
            let line = if text.is_empty() {
                div().into_any_element()
            } else {
                StyledText::new(text).with_runs(runs).into_any_element()
            };
            div().id(ix).h(px(LINE_HEIGHT)).whitespace_nowrap().overflow_hidden().child(line)
        });

        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .font(font)
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .text_color(foreground)
            .child(
                // Measures the text area so the grid follows the pane's size.
                canvas(
                    move |bounds, _, cx| {
                        let columns = (bounds.size.width / cell_width).floor().max(2.) as u16;
                        let lines = (bounds.size.height / px(LINE_HEIGHT)).floor().max(1.) as u16;
                        _ = view.update(cx, |view, cx| {
                            view.origin = bounds.origin;
                            view.set_grid(columns, lines, cx);
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(lines)
            .when(show_cursor, |this| {
                let cursor = div()
                    .absolute()
                    .left(cell_width * cursor.point.column.0 as f32)
                    .top(px(LINE_HEIGHT) * cursor_row as f32)
                    .w(cell_width)
                    .h(px(LINE_HEIGHT));
                this.child(if focused {
                    cursor.bg(cursor_color.opacity(0.7))
                } else {
                    cursor.border_1().border_color(cursor_color)
                })
            })
            .into_any_element()
    }
}

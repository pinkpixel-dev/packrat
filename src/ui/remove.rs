use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use super::Packrat;
use crate::model::category::Category;
use crate::model::package::{Package, Source, format_size};
use crate::ops::{self, Preview, Target};
use crate::terminal::Command;

const DIALOG_WIDTH: f32 = 520.;
const LIST_MAX_HEIGHT: f32 = 280.;

impl Packrat {
    /// Starts removing one package: a preview dialog first, then the real
    /// command in the terminal pane.
    pub(super) fn remove_package(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(package) = self.package(ix).cloned() else {
            return;
        };
        if !self.ready_to_run(window, cx) {
            return;
        }
        match &package.source {
            Source::Flatpak { .. } => self.confirm_flatpak_removal(&package, window, cx),
            Source::Repo(_) | Source::Aur => {
                let title = format!("Remove {}?", package.display_name());
                self.preview_and_confirm(vec![package.name.clone()], title, window, cx);
            }
        }
    }

    /// Removes every orphan in one pacman transaction.
    pub(super) fn remove_orphans(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inventory) = self.inventory() else {
            return;
        };
        let names: Vec<String> = inventory
            .packages
            .iter()
            .filter(|package| package.orphan)
            .map(|package| package.name.clone())
            .collect();
        if names.is_empty() || !self.ready_to_run(window, cx) {
            return;
        }
        let title = format!("Remove {} orphans?", names.len());
        self.preview_and_confirm(names, title, window, cx);
    }

    /// Removes Flatpak runtimes no app uses anymore. Flatpak shows the list
    /// and asks in the pane, so there's no preview dialog here.
    pub(super) fn remove_unused_runtimes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inventory) = self.inventory() else {
            return;
        };
        let mut installations: Vec<&str> = inventory
            .packages
            .iter()
            .filter(|package| package.category == Category::Runtimes)
            .filter_map(|package| match &package.source {
                Source::Flatpak { installation } => Some(installation.as_str()),
                _ => None,
            })
            .collect();
        installations.sort();
        installations.dedup();
        let Some(command) = ops::flatpak_remove_unused(&installations) else {
            return;
        };
        if self.ready_to_run(window, cx) {
            self.run_command(command, window, cx);
        }
    }

    /// One command at a time: pacman holds a lock, and so does the pane.
    pub(super) fn ready_to_run(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.terminal.read(cx).is_running(cx) {
            window.push_notification(Notification::warning("Wait for the current command to finish first."), cx);
            return false;
        }
        !window.has_active_dialog(cx)
    }

    fn preview_and_confirm(&mut self, names: Vec<String>, title: String, window: &mut Window, cx: &mut Context<Self>) {
        let preview = cx.background_spawn({
            let names = names.clone();
            async move { ops::preview_pacman_removal(&names) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let preview = preview.await;
            _ = this.update_in(cx, |this, window, cx| match preview {
                Preview::Removes(targets) => {
                    let command = ops::pacman_remove(&names);
                    this.open_confirm(title.into(), targets, command, window, cx);
                }
                Preview::Blocked(reason) => open_blocked(title.into(), reason.into(), window, cx),
            });
        })
        .detach();
    }

    fn confirm_flatpak_removal(&mut self, package: &Package, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = ops::flatpak_remove(package) else {
            return;
        };
        let targets = vec![Target {
            name: package.name.clone(),
            version: package.version.clone(),
            size: package.size,
        }];
        let title = format!("Remove {}?", package.display_name());
        self.open_confirm(title.into(), targets, command, window, cx);
    }

    fn open_confirm(
        &mut self,
        title: SharedString,
        targets: Vec<Target>,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.entity().downgrade();
        let total = format_size(Preview::total_size(&targets));
        let summary: SharedString = match targets.len() {
            1 => format!("Frees {total}.").into(),
            count => format!("{count} packages, frees {total}.").into(),
        };
        let command_line: SharedString = command.display().into();

        window.open_dialog(cx, move |dialog, _, cx| {
            let theme = cx.theme();
            let rows = targets.iter().map(|target| {
                h_flex()
                    .gap_3()
                    .py_0p5()
                    .text_sm()
                    .child(div().flex_1().min_w_0().truncate().child(target.name.clone()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(target.version.clone()))
                    .child(
                        div()
                            .w(px(72.))
                            .text_right()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format_size(target.size)),
                    )
            });
            let run = this.clone();
            let command = command.clone();

            dialog
                .w(px(DIALOG_WIDTH))
                .title(title.clone())
                .child(
                    v_flex()
                        .gap_3()
                        .child(
                            v_flex()
                                .id("remove-targets")
                                .max_h(px(LIST_MAX_HEIGHT))
                                .overflow_y_scrollbar()
                                .children(rows),
                        )
                        .child(div().text_sm().text_color(theme.muted_foreground).child(summary.clone()))
                        .child(command_box(command_line.clone(), cx)),
                )
                .footer(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("remove-cancel")
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(Button::new("remove-confirm").danger().label("Remove").on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            let command = command.clone();
                            _ = run.update(cx, |this, cx| this.run_command(command, window, cx));
                        })),
                )
        });
    }
}

/// Shown when pacman won't remove something, with its reason.
fn open_blocked(title: SharedString, reason: SharedString, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, _, cx| {
        dialog
            .w(px(DIALOG_WIDTH))
            .title(title.clone())
            .child(
                v_flex()
                    .gap_3()
                    .child(div().text_sm().child("pacman stopped before removing anything. Here's what it said:"))
                    .child(command_box(reason.clone(), cx)),
            )
            .footer(
                h_flex().w_full().justify_end().child(
                    Button::new("blocked-close")
                        .outline()
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
    });
}

/// Monospace text in a dark box: the exact command, or pacman's own words.
fn command_box(text: SharedString, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .p_2()
        .rounded(theme.radius)
        .bg(theme.group_box)
        .border_1()
        .border_color(theme.border)
        .font_family(theme.mono_font_family.clone())
        .text_xs()
        .text_color(theme.muted_foreground)
        .whitespace_normal()
        .child(text)
}

use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::Packrat;
use crate::model::package::Source;
use crate::model::update::Key;
use crate::ops;
use crate::settings::AurHelper;
use crate::sources;
use crate::terminal::Command;

impl Packrat {
    /// The AUR helper commands use right now.
    pub(super) fn aur_helper(&self) -> Option<AurHelper> {
        self.settings.aur_helper(&self.aur_helpers)
    }

    /// Looks for pending updates off the UI thread. The previous results stay
    /// listed until the new ones arrive.
    pub(super) fn check_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.checking_updates {
            return;
        }
        self.checking_updates = true;
        let helper = self.aur_helper();
        let task = cx.background_spawn(async move { sources::updates::check(helper) });
        cx.spawn_in(window, async move |this, cx| {
            let check = task.await;
            _ = this.update_in(cx, |this, window, cx| {
                this.checking_updates = false;
                this.updates = Rc::new(check.updates);
                if !check.errors.is_empty() {
                    window.push_notification(Notification::warning(check.errors.join("\n")), cx);
                }
                this.refilter(cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// Updates everything with a pending update, in one run in the pane.
    pub(super) fn update_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inventory) = self.inventory().cloned() else {
            return;
        };
        let helper = self.aur_helper();
        let (mut repo, mut aur) = (false, false);
        let mut installations: Vec<&str> = Vec::new();
        let mut keys = Vec::new();
        for package in inventory.packages.iter().filter(|package| self.updates.get(package).is_some()) {
            match &package.source {
                Source::Repo(_) => repo = true,
                // Without a helper there's nothing to rebuild AUR packages with.
                Source::Aur if helper.is_none() => continue,
                Source::Aur => aur = true,
                Source::Flatpak { installation } => installations.push(installation),
            }
            keys.push(Key::of(package));
        }
        installations.sort();
        installations.dedup();
        let Some(command) = ops::update_all(repo, helper.filter(|_| aur), &installations) else {
            return;
        };
        // Full upgrades keep the pane open: pacman's notes about .pacnew files
        // and the like are worth reading.
        self.start_update(command, keys, false, window, cx);
    }

    /// Updates one package. Repo packages can't update alone without a
    /// partial upgrade, so for them this is a full system update.
    pub(super) fn update_package(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(package) = self.package(ix).cloned() else {
            return;
        };
        match &package.source {
            Source::Repo(_) => self.update_system(window, cx),
            Source::Aur => {
                let Some(helper) = self.aur_helper() else {
                    window.push_notification(Notification::warning("Install paru or yay to update AUR packages."), cx);
                    return;
                };
                self.start_update(ops::aur_update(helper, &package.name), vec![Key::of(&package)], true, window, cx);
            }
            Source::Flatpak { .. } => {
                if let Some(command) = ops::flatpak_update(&package) {
                    self.start_update(command, vec![Key::of(&package)], true, window, cx);
                }
            }
        }
    }

    /// `sudo pacman -Syu`, covering every pending repo update.
    fn update_system(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self
            .inventory()
            .map(|inventory| {
                inventory
                    .packages
                    .iter()
                    .filter(|package| matches!(package.source, Source::Repo(_)) && self.updates.get(package).is_some())
                    .map(Key::of)
                    .collect()
            })
            .unwrap_or_default();
        self.start_update(ops::system_update(), keys, false, window, cx);
    }

    fn start_update(&mut self, command: Command, keys: Vec<Key>, auto_close: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.ready_to_run(window, cx) && self.run_in_pane(command, auto_close, window, cx) {
            self.updating = Some(keys);
        }
    }
}

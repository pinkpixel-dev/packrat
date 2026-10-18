use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Packrat;
use super::details::{DETAILS_WIDTH, fact_row};
use super::package_list::{ROW_HEIGHT, source_tag};
use crate::model::candidate::Candidate;
use crate::model::package::{Source, SourceKind};
use crate::ops;
use crate::sources::search::{self, MIN_QUERY};
use crate::terminal::Command;

/// How long typing has to pause before a search goes out.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// The Find view: search results for packages you don't have yet.
#[derive(Default)]
pub(super) struct Find {
    /// Results for `query`, best matches first.
    results: Rc<Vec<Candidate>>,
    /// Indices into `results` that pass the source filter, in display order.
    pub(super) visible: Rc<Vec<usize>>,
    /// The selected result, as an index into `results`.
    pub(super) selected: Option<usize>,
    /// The query `results` belong to.
    query: String,
    pub(super) searching: bool,
    errors: Vec<String>,
    /// Bumped by every new query, so a slower search can't land on top of
    /// a newer one.
    generation: u64,
}

impl Packrat {
    /// Searches for the current query once typing pauses.
    pub(super) fn search_soon(&mut self, cx: &mut Context<Self>) {
        let query = self.filter.query.trim().to_string();
        if query == self.find.query && !self.find.searching {
            return;
        }
        self.find.generation += 1;
        let generation = self.find.generation;
        if query.chars().count() < MIN_QUERY {
            self.find = Find { generation, ..Find::default() };
            cx.notify();
            return;
        }
        self.find.searching = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(DEBOUNCE).await;
            let stale = this.read_with(cx, |this, _| this.find.generation != generation).unwrap_or(true);
            if stale {
                return;
            }
            let found = cx.background_executor().spawn({
                let query = query.clone();
                async move { search::search(&query) }
            });
            let found = found.await;
            _ = this.update(cx, |this, cx| {
                if this.find.generation != generation {
                    return;
                }
                this.find.results = Rc::new(found.candidates);
                this.find.errors = found.errors;
                this.find.query = query;
                this.find.searching = false;
                this.find.selected = None;
                this.refilter_find();
                this.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Applies the source filter to the results, keeping the selection when
    /// it's still listed.
    pub(super) fn refilter_find(&mut self) {
        let source = self.filter.source;
        self.find.visible = Rc::new(
            self.find
                .results
                .iter()
                .enumerate()
                .filter(|(_, candidate)| source.is_none_or(|kind| candidate.source.kind() == kind))
                .map(|(ix, _)| ix)
                .collect(),
        );
        if !self.find.selected.is_some_and(|ix| self.find.visible.contains(&ix)) {
            self.find.selected = self.find.visible.first().copied();
        }
    }

    /// Whether a result is already installed, matching pacman names against
    /// pacman packages and Flatpak IDs against Flatpaks.
    fn is_installed(&self, candidate: &Candidate) -> bool {
        let flatpak = candidate.source.kind() == SourceKind::Flatpak;
        self.inventory().is_some_and(|inventory| {
            inventory.packages.iter().any(|package| {
                package.name == candidate.name && (package.source.kind() == SourceKind::Flatpak) == flatpak
            })
        })
    }

    /// The command that installs a result, and whether it's a full system
    /// upgrade. `None` for AUR packages when there's no helper to build with.
    fn install_command(&self, candidate: &Candidate) -> Option<(Command, bool)> {
        match &candidate.source {
            Source::Repo(_) => {
                // Installing from an old sync DB can fail or skew library
                // versions, so a system that's behind gets upgraded too.
                let behind = !self.pending_repo_keys().is_empty();
                Some((ops::pacman_install(&candidate.name, behind), behind))
            }
            Source::Aur => self.aur_helper().map(|helper| (ops::aur_install(helper, &candidate.name), false)),
            Source::Flatpak { installation } => {
                let remote = candidate.remote.as_deref()?;
                Some((ops::flatpak_install(remote, &candidate.name, installation), false))
            }
        }
    }

    /// Installs a result in the terminal pane. pacman, the AUR helper and
    /// Flatpak each show what they'll do and ask before doing it.
    fn install(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(candidate) = self.find.results.get(ix).cloned() else {
            return;
        };
        let Some((command, full_upgrade)) = self.install_command(&candidate) else {
            window.push_notification(Notification::warning("Install paru or yay to build AUR packages."), cx);
            return;
        };
        if full_upgrade {
            // Same as Update System: the pane stays open, and the repo
            // updates it covers clear when it succeeds.
            let keys = self.pending_repo_keys();
            self.start_update(command, keys, false, window, cx);
        } else if self.ready_to_run(window, cx) && self.run_in_pane(command, true, window, cx) {
            self.repo_install = matches!(candidate.source, Source::Repo(_));
        }
    }

    /// The results list, or a message when there's nothing to list.
    pub(super) fn render_find_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let visible = self.find.visible.clone();
        if visible.is_empty() {
            let text = if self.filter.query.trim().chars().count() < MIN_QUERY {
                "Search the repos, the AUR and Flatpak"
            } else if self.find.searching {
                "Searching…"
            } else {
                "No packages found"
            };
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text)
                .into_any_element();
        }

        let entity = cx.entity().downgrade();
        div()
            .relative()
            .size_full()
            .child(
                uniform_list("candidates", visible.len(), move |range, _, cx| {
                    let Some(strong) = entity.upgrade() else {
                        return Vec::new();
                    };
                    let this = strong.read(cx);
                    range
                        .filter_map(|row| {
                            let ix = *this.find.visible.get(row)?;
                            let candidate = this.find.results.get(ix)?;
                            let installed = this.is_installed(candidate);
                            let selected = this.find.selected == Some(ix);
                            Some(render_row(row, candidate, installed, selected, entity.clone(), cx))
                        })
                        .collect()
                })
                .track_scroll(&self.list_scroll)
                .size_full(),
            )
            .vertical_scrollbar(&self.list_scroll)
            .into_any_element()
    }

    /// Sources that couldn't be searched, one line each, under the header.
    pub(super) fn render_find_errors(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.find.errors.is_empty() {
            return None;
        }
        let theme = cx.theme();
        Some(
            v_flex()
                .flex_shrink_0()
                .px_4()
                .py_2()
                .gap_0p5()
                .border_b_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.warning)
                .children(self.find.errors.iter().map(|error| div().truncate().child(error.clone())))
                .into_any_element(),
        )
    }

    pub(super) fn render_candidate_details(&self, cx: &mut Context<Self>) -> AnyElement {
        let panel = v_flex()
            .id("candidate-details")
            .w(px(DETAILS_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        let Some((ix, candidate)) = self.find.selected.and_then(|ix| Some((ix, self.find.results.get(ix)?))) else {
            return panel
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Select a package")
                .into_any_element();
        };

        let installed = self.is_installed(candidate);
        let install = self.install_command(candidate);
        let action = if installed {
            let name = candidate.name.clone();
            Button::new("show-installed")
                .outline()
                .small()
                .label("Show in Library")
                .tooltip("Already installed")
                .on_click(cx.listener(move |this, _, window, cx| this.reveal(&name, window, cx)))
        } else {
            let tooltip = match &install {
                Some((command, _)) => command.display(),
                None => "Install paru or yay to build AUR packages".to_string(),
            };
            Button::new("install-package")
                .primary()
                .small()
                .icon(Icon::new(CatalogIcon::Download))
                .label("Install")
                .tooltip(tooltip)
                .disabled(install.is_none())
                .on_click(cx.listener(move |this, _, window, cx| this.install(ix, window, cx)))
        };
        let upgrades_too = !installed && install.is_some_and(|(_, full_upgrade)| full_upgrade);

        panel
            .p_5()
            .gap_5()
            .overflow_y_scrollbar()
            .child(header(candidate, installed, cx))
            .child(
                v_flex().gap_2().child(h_flex().child(action)).when(upgrades_too, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Updates are waiting, so this upgrades the whole system too."),
                    )
                }),
            )
            .child(facts(candidate, cx))
            .into_any_element()
    }
}

fn render_row(
    row: usize,
    candidate: &Candidate,
    installed: bool,
    selected: bool,
    entity: WeakEntity<Packrat>,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let name = candidate.display_name().to_string();
    // Flatpaks show their app ID too.
    let subtitle = (name != candidate.name).then(|| candidate.name.clone());

    h_flex()
        .id(row)
        .h(px(ROW_HEIGHT))
        .w_full()
        .px_4()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(theme.border.opacity(0.5))
        .cursor_pointer()
        .when(selected, |this| this.bg(theme.list_active))
        .when(!selected, |this| this.hover(|style| style.bg(theme.list_hover)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    h_flex()
                        .gap_2()
                        .items_baseline()
                        .min_w_0()
                        .child(div().truncate().text_sm().font_medium().child(name))
                        .when_some(subtitle, |this, subtitle| {
                            this.child(div().truncate().text_xs().text_color(theme.muted_foreground).child(subtitle))
                        })
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(candidate.version.clone()),
                        ),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(candidate.description.clone()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(72.))
                .flex()
                .justify_end()
                .when(installed, |this| this.child(Tag::success().xsmall().outline().child("Installed"))),
        )
        .child(div().flex_shrink_0().w(px(124.)).flex().justify_end().child(source_tag(&candidate.source)))
        .on_click(move |_, window, cx| {
            _ = entity.update(cx, |this, cx| {
                this.select_row(row, cx);
                this.focus_handle.focus(window, cx);
            });
        })
        .into_any_element()
}

fn header(candidate: &Candidate, installed: bool, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let name = candidate.display_name().to_string();
    let out_of_date = candidate.aur.as_ref().is_some_and(|aur| aur.out_of_date);

    v_flex()
        .gap_2()
        .child(div().text_lg().font_semibold().child(name.clone()))
        .when(name != candidate.name, |this| {
            this.child(div().text_xs().text_color(theme.muted_foreground).child(candidate.name.clone()))
        })
        .child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .child(source_tag(&candidate.source))
                .when(installed, |this| this.child(Tag::success().xsmall().outline().child("Installed")))
                .when(out_of_date, |this| this.child(Tag::danger().xsmall().outline().child("Out of Date"))),
        )
        .when(!candidate.description.is_empty(), |this| {
            this.child(div().text_sm().text_color(theme.muted_foreground).child(candidate.description.clone()))
        })
}

fn facts(candidate: &Candidate, cx: &App) -> impl IntoElement {
    let mut rows: Vec<(&str, AnyElement)> = vec![("Version", candidate.version.clone().into_any_element())];
    let mut links: Vec<(&str, String)> = Vec::new();
    match &candidate.source {
        Source::Repo(repo) => rows.push(("Repository", repo.clone().into_any_element())),
        Source::Aur => {
            rows.push(("Repository", "AUR".into_any_element()));
            links.push(("AUR Page", format!("https://aur.archlinux.org/packages/{}", candidate.name)));
        }
        Source::Flatpak { installation } => {
            let remote = candidate.remote.clone().unwrap_or_default();
            if remote == "flathub" {
                links.push(("Flathub", format!("https://flathub.org/apps/{}", candidate.name)));
            }
            rows.push(("Remote", remote.into_any_element()));
            rows.push(("Installs to", installation.clone().into_any_element()));
        }
    }
    if let Some(aur) = &candidate.aur {
        rows.push(("Votes", aur.votes.to_string().into_any_element()));
        rows.push(("Popularity", format!("{:.2}", aur.popularity).into_any_element()));
        let maintainer = aur.maintainer.clone().unwrap_or_else(|| "None (orphaned)".into());
        rows.push(("Maintainer", maintainer.into_any_element()));
        if let Some(url) = &aur.url {
            links.insert(0, ("Website", url.clone()));
        }
    }
    rows.extend(links.into_iter().map(|(label, url)| (label, link(label, url, cx))));

    v_flex()
        .gap_1p5()
        .children(rows.into_iter().map(|(label, value)| fact_row(label, value, cx)))
}

fn link(id: &str, url: String, cx: &App) -> AnyElement {
    let theme = cx.theme();
    div()
        .id(SharedString::from(format!("link-{id}")))
        .truncate()
        .text_color(theme.link)
        .cursor_pointer()
        .hover(|style| style.text_color(theme.link_hover))
        .child(url.clone())
        .on_click(move |_, _, cx| cx.open_url(&url))
        .into_any_element()
}

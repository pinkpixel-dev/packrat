mod details;
mod find;
mod package_list;
mod remove;
mod settings;
mod sidebar;
mod title_bar;
mod updates;

use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::resizable::{resizable_panel, v_resizable};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::model::category::Category;
use crate::model::filter::{Filter, Sort, View};
use crate::model::inventory::Inventory;
use crate::model::package::{Package, SourceKind, strip_constraint};
use crate::model::update::{Key, Updates};
use crate::settings::{AurHelper, Settings};
use crate::sources;
use crate::terminal::{Command, PaneEvent, Status, TerminalView};

/// Starting height of the terminal pane.
const TERMINAL_HEIGHT: f32 = 300.;
/// How long a finished command stays on screen before the pane closes itself.
const AUTO_CLOSE_DELAY: std::time::Duration = std::time::Duration::from_millis(1500);

const KEY_CONTEXT: &str = "Packrat";
/// Active only while the package list has focus, so single-letter keys never
/// reach it while you're typing in search.
const LIST_CONTEXT: &str = "PackageList";

actions!(
    packrat,
    [
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        NextView,
        PreviousView,
        FocusSearch,
        FocusList,
        ToggleTerminal,
        Refresh,
        Quit
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(LIST_CONTEXT)),
        KeyBinding::new("j", SelectNext, Some(LIST_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(LIST_CONTEXT)),
        KeyBinding::new("k", SelectPrevious, Some(LIST_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(LIST_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(LIST_CONTEXT)),
        KeyBinding::new("/", FocusSearch, Some(LIST_CONTEXT)),
        KeyBinding::new("alt-down", NextView, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-up", PreviousView, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusSearch, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-l", FocusList, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-`", ToggleTerminal, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-r", Refresh, Some(KEY_CONTEXT)),
        KeyBinding::new("f5", Refresh, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-q", Quit, Some(KEY_CONTEXT)),
    ]);
}

enum Load {
    Loading,
    Ready(Rc<Inventory>),
    Failed(SharedString),
}

/// The main window: sidebar, package list and details.
pub struct Packrat {
    focus_handle: FocusHandle,
    search: Entity<InputState>,
    load: Load,
    filter: Filter,
    /// Indices into the inventory, in display order.
    visible: Rc<Vec<usize>>,
    /// The selected package, as an inventory index.
    selected: Option<usize>,
    list_scroll: UniformListScrollHandle,
    terminal: Entity<TerminalView>,
    terminal_open: bool,
    settings: Settings,
    /// AUR helpers found on `PATH` at startup.
    aur_helpers: Vec<AurHelper>,
    updates: Rc<Updates>,
    checking_updates: bool,
    /// What the running update command covers, cleared from `updates` when it succeeds.
    updating: Option<Vec<Key>>,
    /// Whether the running command is a plain `pacman -S` install, which
    /// fails when the sync DB is out of date.
    repo_install: bool,
    find: find::Find,
    _subscriptions: Vec<Subscription>,
}

impl Packrat {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search packages"));
        let terminal = cx.new(TerminalView::new);
        let mut subscriptions = vec![cx.subscribe_in(&search, window, |this, search, event: &InputEvent, window, cx| {
            match event {
                InputEvent::Change => {
                    this.filter.query = search.read(cx).value().to_string();
                    this.refilter(cx);
                    if this.filter.view == View::Find {
                        this.search_soon(cx);
                    }
                }
                // Enter hands the keyboard to the list, ready for arrow keys.
                InputEvent::PressEnter { .. } => this.focus_handle.focus(window, cx),
                _ => {}
            }
        })];
        subscriptions.push(cx.subscribe_in(&terminal, window, |this, _, event: &PaneEvent, window, cx| match event {
            // Whatever ran probably changed what's installed.
            PaneEvent::Finished { status, label, auto_close } => {
                let updated = this.updating.take();
                let repo_install = std::mem::take(&mut this.repo_install);
                if *status != Status::Exited(Some(0)) {
                    // Failures stay open so the error is still on screen.
                    if repo_install {
                        window.push_notification(
                            Notification::warning(
                                "If pacman couldn't download it, your package lists are out of date. Update the system, then try again.",
                            ),
                            cx,
                        );
                    }
                    return;
                }
                if let Some(keys) = updated {
                    let updates = Rc::make_mut(&mut this.updates);
                    keys.iter().for_each(|key| updates.remove(key));
                }
                this.reload(cx);
                if *auto_close {
                    window.push_notification(Notification::success(format!("Done: {label}")), cx);
                    this.close_terminal_soon(window, cx);
                }
            }
            PaneEvent::Hide => {
                this.terminal_open = false;
                this.focus_handle.focus(window, cx);
                cx.notify();
            }
        }));

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        let mut this = Self {
            focus_handle,
            search,
            load: Load::Loading,
            filter: Filter::default(),
            visible: Rc::default(),
            selected: None,
            list_scroll: UniformListScrollHandle::new(),
            terminal,
            terminal_open: false,
            settings: Settings::load(),
            aur_helpers: AurHelper::installed(),
            updates: Rc::default(),
            checking_updates: false,
            updating: None,
            repo_install: false,
            find: find::Find::default(),
            _subscriptions: subscriptions,
        };
        this.reload(cx);
        // Once at startup; after that only the Check button looks again.
        this.check_updates(window, cx);
        this
    }

    fn inventory(&self) -> Option<&Rc<Inventory>> {
        match &self.load {
            Load::Ready(inventory) => Some(inventory),
            _ => None,
        }
    }

    fn package(&self, ix: usize) -> Option<&Package> {
        self.inventory()?.packages.get(ix)
    }

    /// Reads everything installed again, off the UI thread. The selection
    /// survives by package name.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let selected_name = self.selected.and_then(|ix| self.package(ix)).map(|package| package.name.clone());
        if self.inventory().is_none() {
            self.load = Load::Loading;
        }
        let task = cx.background_spawn(async { sources::load_inventory() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                match result {
                    Ok(inventory) => {
                        this.selected = selected_name
                            .and_then(|name| inventory.packages.iter().position(|package| package.name == name));
                        this.load = Load::Ready(Rc::new(inventory));
                    }
                    Err(err) => this.load = Load::Failed(err.into()),
                }
                this.refilter(cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        self.visible = Rc::new(match self.inventory() {
            Some(inventory) => self.filter.apply(&inventory.packages, &self.updates),
            None => Vec::new(),
        });
        self.refilter_find();
        // Keep the selection when it's still listed, otherwise start at the top.
        if !self.selected.is_some_and(|ix| self.visible.contains(&ix)) {
            self.selected = self.visible.first().copied();
            self.list_scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        self.filter.view = view;
        self.refilter(cx);
        if view == View::Find {
            self.search_soon(cx);
        }
    }

    fn set_source(&mut self, source: Option<SourceKind>, cx: &mut Context<Self>) {
        self.filter.source = source;
        self.refilter(cx);
    }

    fn set_sort(&mut self, sort: Sort, cx: &mut Context<Self>) {
        self.filter.sort = sort;
        self.refilter(cx);
    }

    /// Every sidebar view in order, skipping empty categories like the sidebar does.
    fn sidebar_views(&self) -> Vec<View> {
        let mut views = vec![View::Find, View::Apps, View::All, View::Explicit, View::Orphans, View::Updates];
        if let Some(inventory) = self.inventory() {
            views.extend(
                Category::ALL
                    .into_iter()
                    .map(View::Category)
                    .filter(|&view| inventory.packages.iter().any(|package| Filter::in_view(view, package, &self.updates))),
            );
        }
        views
    }

    fn step_view(&mut self, forward: bool, cx: &mut Context<Self>) {
        let views = self.sidebar_views();
        let current = views.iter().position(|&view| view == self.filter.view).unwrap_or(0);
        let next = if forward {
            (current + 1) % views.len()
        } else {
            (current + views.len() - 1) % views.len()
        };
        self.set_view(views[next], cx);
    }

    fn next_view(&mut self, _: &NextView, _: &mut Window, cx: &mut Context<Self>) {
        self.step_view(true, cx);
    }

    fn previous_view(&mut self, _: &PreviousView, _: &mut Window, cx: &mut Context<Self>) {
        self.step_view(false, cx);
    }

    /// Selects a package by name or by something it provides (`sh` finds
    /// bash), widening the filters when they hide it.
    fn reveal(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inventory) = self.inventory() else {
            return;
        };
        let found = inventory
            .packages
            .iter()
            .position(|package| package.name == name)
            .or_else(|| {
                inventory.packages.iter().position(|package| {
                    package.provides.iter().any(|provided| strip_constraint(provided) == name)
                })
            });
        let Some(ix) = found else {
            return;
        };
        if !self.visible.contains(&ix) {
            self.filter.view = View::All;
            self.filter.source = None;
            if !self.filter.query.is_empty() {
                self.filter.query.clear();
                self.search.update(cx, |search, cx| search.set_value("", window, cx));
            }
        }
        self.selected = Some(ix);
        self.refilter(cx);
        if let Some(row) = self.selected_row() {
            self.list_scroll.scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    /// What the list shows right now: inventory indices, or result indices
    /// in Find.
    fn rows(&self) -> &Rc<Vec<usize>> {
        if self.filter.view == View::Find { &self.find.visible } else { &self.visible }
    }

    fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(&ix) = self.rows().get(row) {
            if self.filter.view == View::Find {
                self.find.selected = Some(ix);
            } else {
                self.selected = Some(ix);
            }
            self.list_scroll.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn selected_row(&self) -> Option<usize> {
        let selected = if self.filter.view == View::Find { self.find.selected } else { self.selected }?;
        self.rows().iter().position(|&ix| ix == selected)
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.selected_row().map_or(0, |row| (row + 1).min(self.rows().len().saturating_sub(1)));
        self.select_row(row, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.selected_row().map_or(0, |row| row.saturating_sub(1));
        self.select_row(row, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select_row(0, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        self.select_row(self.rows().len().saturating_sub(1), cx);
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| search.focus(window, cx));
    }

    fn focus_list(&mut self, _: &FocusList, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
    }

    /// Opens the terminal pane and runs `command` in it.
    pub(super) fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        self.run_in_pane(command, true, window, cx);
    }

    /// Like [`Self::run_command`], choosing whether the pane closes itself
    /// after success. Returns whether the command started.
    fn run_in_pane(&mut self, command: Command, auto_close: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let label = command.display();
        let result = self.terminal.update(cx, |terminal, cx| terminal.run(command, auto_close, window, cx));
        match result {
            Ok(()) => {
                self.terminal_open = true;
                cx.notify();
                true
            }
            Err(err) => {
                window.push_notification(Notification::error(format!("Couldn't run {label}: {err}")), cx);
                false
            }
        }
    }

    /// Hides the pane a moment after a command succeeds, long enough to see
    /// "Done". Skipped if something else started running in the meantime.
    fn close_terminal_soon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(AUTO_CLOSE_DELAY).await;
            _ = this.update_in(cx, |this, window, cx| {
                if this.terminal_open && !this.terminal.read(cx).is_running(cx) {
                    this.terminal_open = false;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn toggle_terminal(&mut self, _: &ToggleTerminal, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_open = !self.terminal_open;
        if self.terminal_open {
            self.terminal.read(cx).focus_handle(cx).focus(window, cx);
        } else {
            self.focus_handle.focus(window, cx);
        }
        cx.notify();
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    fn quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        cx.quit();
    }

    fn render_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match &self.load {
            Load::Loading => centered_message("Reading installed packages…", cx),
            Load::Failed(err) => centered_message(err.clone(), cx),
            Load::Ready(_) => {
                let browser = h_flex()
                    .size_full()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_list(window, cx))
                    .child(self.render_details(cx));
                v_resizable("layout")
                    .child(resizable_panel().child(browser))
                    .child(
                        resizable_panel()
                            .size(px(TERMINAL_HEIGHT))
                            .size_range(px(120.)..px(1600.))
                            .visible(self.terminal_open)
                            .child(self.terminal.clone()),
                    )
                    .into_any_element()
            }
        }
    }
}

fn centered_message(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
        .into_any_element()
}

impl Focusable for Packrat {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Packrat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(Self::focus_search))
            .on_action(cx.listener(Self::next_view))
            .on_action(cx.listener(Self::previous_view))
            .on_action(cx.listener(Self::focus_list))
            .on_action(cx.listener(Self::toggle_terminal))
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::quit))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(div().flex_1().min_h_0().child(self.render_body(window, cx)))
    }
}

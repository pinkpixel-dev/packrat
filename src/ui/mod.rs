mod details;
mod package_list;
mod sidebar;
mod title_bar;

use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use crate::model::category::Category;
use crate::model::filter::{Filter, Sort, View};
use crate::model::inventory::Inventory;
use crate::model::package::{Package, SourceKind, strip_constraint};
use crate::sources;

const KEY_CONTEXT: &str = "Packrat";
/// Active only while the package list has focus, so single-letter keys never
/// reach it while you're typing in search.
const LIST_CONTEXT: &str = "PackageList";

actions!(
    packrat,
    [SelectNext, SelectPrevious, SelectFirst, SelectLast, NextView, PreviousView, FocusSearch, FocusList, Refresh, Quit]
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
    _subscriptions: Vec<Subscription>,
}

impl Packrat {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search packages"));
        let subscriptions = vec![cx.subscribe_in(&search, window, |this, search, event: &InputEvent, window, cx| {
            match event {
                InputEvent::Change => {
                    this.filter.query = search.read(cx).value().to_string();
                    this.refilter(cx);
                }
                // Enter hands the keyboard to the list, ready for arrow keys.
                InputEvent::PressEnter { .. } => this.focus_handle.focus(window, cx),
                _ => {}
            }
        })];

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
            _subscriptions: subscriptions,
        };
        this.reload(cx);
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
            Some(inventory) => self.filter.apply(&inventory.packages),
            None => Vec::new(),
        });
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
        let mut views = vec![View::Apps, View::All, View::Explicit, View::Orphans];
        if let Some(inventory) = self.inventory() {
            views.extend(
                Category::ALL
                    .into_iter()
                    .map(View::Category)
                    .filter(|&view| inventory.packages.iter().any(|package| Filter::in_view(view, package))),
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

    fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(&ix) = self.visible.get(row) {
            self.selected = Some(ix);
            self.list_scroll.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        }
    }

    fn selected_row(&self) -> Option<usize> {
        let selected = self.selected?;
        self.visible.iter().position(|&ix| ix == selected)
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.selected_row().map_or(0, |row| (row + 1).min(self.visible.len().saturating_sub(1)));
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
        self.select_row(self.visible.len().saturating_sub(1), cx);
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| search.focus(window, cx));
    }

    fn focus_list(&mut self, _: &FocusList, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
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
            Load::Ready(_) => h_flex()
                .size_full()
                .min_h_0()
                .child(self.render_sidebar(cx))
                .child(self.render_list(window, cx))
                .child(self.render_details(cx))
                .into_any_element(),
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
            .on_action(cx.listener(Self::refresh))
            .on_action(cx.listener(Self::quit))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(div().flex_1().min_h_0().child(self.render_body(window, cx)))
    }
}

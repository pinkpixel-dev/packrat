use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Packrat;
use crate::model::category::Category;
use crate::model::filter::{Filter, View};

const SIDEBAR_WIDTH: f32 = 236.;
const ROW_HEIGHT: f32 = 30.;

impl Packrat {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(inventory) = self.inventory() else {
            return div().into_any_element();
        };
        // Counts follow the source filter so they match what the list shows.
        let count = |view: View| {
            inventory
                .packages
                .iter()
                .filter(|package| self.filter.source.is_none_or(|kind| package.source.kind() == kind))
                .filter(|package| Filter::in_view(view, package, &self.updates))
                .count()
        };

        let library = [
            (View::Apps, CatalogIcon::LayoutGrid, "Apps"),
            (View::All, CatalogIcon::Package, "All Packages"),
            (View::Explicit, CatalogIcon::Pin, "Installed by You"),
            (View::Orphans, CatalogIcon::Unlink, "Orphans"),
            (View::Updates, CatalogIcon::CircleArrowUp, "Updates"),
        ];
        let find_row = self.sidebar_row(View::Find, CatalogIcon::PackageSearch, "Find", None, cx);
        let library_rows: Vec<AnyElement> = library
            .into_iter()
            .map(|(view, icon, label)| self.sidebar_row(view, icon, label, Some(count(view)), cx))
            .collect();
        let category_rows: Vec<AnyElement> = Category::ALL
            .into_iter()
            .filter_map(|category| {
                let view = View::Category(category);
                let count = count(view);
                (count > 0).then(|| self.sidebar_row(view, category.icon(), category.label(), Some(count), cx))
            })
            .collect();

        v_flex()
            .id("sidebar")
            .w(px(SIDEBAR_WIDTH))
            .flex_shrink_0()
            .h_full()
            .px_2()
            .py_3()
            .gap_0p5()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .overflow_y_scrollbar()
            .child(find_row)
            .child(section_label("Library", cx))
            .children(library_rows)
            .child(section_label("Categories", cx))
            .children(category_rows)
            .into_any_element()
    }

    fn sidebar_row(
        &self,
        view: View,
        icon: CatalogIcon,
        label: &'static str,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.filter.view == view;
        let theme = cx.theme();
        let id = SharedString::from(format!("view-{label}"));

        h_flex()
            .id(id)
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .items_center()
            .rounded(theme.radius)
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| this.bg(theme.sidebar_accent).text_color(theme.sidebar_accent_foreground))
            .when(!selected, |this| {
                this.text_color(theme.sidebar_foreground).hover(|style| style.bg(theme.list_hover))
            })
            .child(Icon::new(icon).small().text_color(if selected { theme.foreground } else { theme.muted_foreground }))
            .child(div().flex_1().min_w_0().truncate().child(label))
            .children(count.map(|count| div().text_xs().text_color(theme.muted_foreground).child(count.to_string())))
            .on_click(cx.listener(move |this, _, _, cx| this.set_view(view, cx)))
            .into_any_element()
    }
}

fn section_label(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_2()
        .pt_4()
        .pb_1()
        .text_xs()
        .font_medium()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

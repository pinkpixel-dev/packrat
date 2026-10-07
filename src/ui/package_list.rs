use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{LIST_CONTEXT, Packrat};
use crate::model::category::Category;
use crate::model::filter::{Sort, View};
use crate::model::package::{Package, Source, SourceKind, format_size};

const ROW_HEIGHT: f32 = 56.;
const HEADER_HEIGHT: f32 = 48.;

impl Packrat {
    pub(super) fn render_list(&self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.visible.clone();
        let entity = cx.entity().downgrade();
        let has_query = !self.filter.query.trim().is_empty();

        let list = if visible.is_empty() {
            let text = if has_query { "No packages match your search" } else { "Nothing here" };
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(text)
                .into_any_element()
        } else {
            div()
                .relative()
                .size_full()
                .child(
                    uniform_list("packages", visible.len(), move |range, _, cx| {
                        let Some(strong) = entity.upgrade() else {
                            return Vec::new();
                        };
                        let this = strong.read(cx);
                        range
                            .filter_map(|row| {
                                let ix = *this.visible.get(row)?;
                                let package = this.package(ix)?;
                                Some(render_row(row, package, this.selected == Some(ix), entity.clone(), cx))
                            })
                            .collect()
                    })
                    .track_scroll(&self.list_scroll)
                    .size_full(),
                )
                .vertical_scrollbar(&self.list_scroll)
                .into_any_element()
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .key_context(LIST_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .child(self.render_list_header(cx))
            .child(div().flex_1().min_h_0().child(list))
    }

    fn render_list_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match self.filter.view {
            View::All => "All Packages",
            View::Apps => "Apps",
            View::Explicit => "Installed by You",
            View::Orphans => "Orphans",
            View::Category(category) => category.label(),
        };
        let sources = [
            (None, "All"),
            (Some(SourceKind::Repo), "Repos"),
            (Some(SourceKind::Aur), "AUR"),
            (Some(SourceKind::Flatpak), "Flatpak"),
        ];
        let current_sort = self.filter.sort;
        let show_remove_orphans = self.filter.view == View::Orphans && !self.visible.is_empty();
        let show_remove_unused =
            self.filter.view == View::Category(Category::Runtimes) && !self.visible.is_empty();
        let this = cx.entity().downgrade();

        h_flex()
            .h(px(HEADER_HEIGHT))
            .flex_shrink_0()
            .px_4()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .items_baseline()
                    .child(div().truncate().font_semibold().child(title))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.visible.len().to_string()),
                    ),
            )
            .when(show_remove_orphans, |this| {
                this.child(
                    Button::new("remove-orphans")
                        .danger()
                        .outline()
                        .small()
                        .flex_shrink_0()
                        .label("Remove All")
                        .tooltip("Remove every orphan in one go")
                        .on_click(cx.listener(|this, _, window, cx| this.remove_orphans(window, cx))),
                )
            })
            .when(show_remove_unused, |this| {
                this.child(
                    Button::new("remove-unused-runtimes")
                        .danger()
                        .outline()
                        .small()
                        .flex_shrink_0()
                        .label("Remove Unused")
                        .tooltip("Remove runtimes no installed app needs")
                        .on_click(cx.listener(|this, _, window, cx| this.remove_unused_runtimes(window, cx))),
                )
            })
            .child(h_flex().gap_1().flex_shrink_0().children(sources.into_iter().map(|(source, label)| {
                Button::new(SharedString::from(format!("source-{label}")))
                    .ghost()
                    .small()
                    .label(label)
                    .selected(self.filter.source == source)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_source(source, cx)))
            })))
            .child(
                Button::new("sort")
                    .ghost()
                    .small()
                    .flex_shrink_0()
                    .icon(Icon::new(CatalogIcon::ArrowDownUp))
                    .label(current_sort.label())
                    .accessibility_label("Sort Packages")
                    .tooltip("Sort")
                    .dropdown_menu(move |mut menu, _, _| {
                        for sort in [Sort::Name, Sort::Size, Sort::Installed] {
                            let this = this.clone();
                            menu = menu.item(PopupMenuItem::new(sort.label()).checked(sort == current_sort).on_click(
                                move |_, _, cx| {
                                    _ = this.update(cx, |this, cx| this.set_sort(sort, cx));
                                },
                            ));
                        }
                        menu
                    }),
            )
    }
}

fn render_row(row: usize, package: &Package, selected: bool, entity: WeakEntity<Packrat>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let name = package.display_name().to_string();
    // Show the package name too when the launcher name hides it.
    let subtitle = (name != package.name).then(|| package.name.clone());

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
                                .child(package.version.clone()),
                        ),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(package.description.clone()),
                ),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(72.))
                .text_right()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format_size(package.size)),
        )
        .child(div().flex_shrink_0().w(px(124.)).flex().justify_end().child(source_tag(&package.source)))
        .on_click(move |_, window, cx| {
            _ = entity.update(cx, |this, cx| {
                this.select_row(row, cx);
                this.focus_handle.focus(window, cx);
            });
        })
        .into_any_element()
}

/// The source badge. AUR and Flatpak get a color so they stand out from the
/// repo packages most of the list is made of.
pub(super) fn source_tag(source: &Source) -> Tag {
    let tag = match source {
        Source::Repo(_) => Tag::secondary(),
        Source::Aur => Tag::warning(),
        Source::Flatpak { .. } => Tag::info(),
    };
    tag.xsmall().outline().child(source.badge().to_string())
}

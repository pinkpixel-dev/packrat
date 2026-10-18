use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tag::Tag;
use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::Packrat;
use super::package_list::source_tag;
use crate::model::package::{Package, Reason, Source, format_date, format_size, strip_constraint};

const DETAILS_WIDTH: f32 = 380.;
/// Long dependency lists (glibc is needed by over a thousand packages) stop
/// here, with a count of the rest.
const MAX_CHIPS: usize = 60;

impl Packrat {
    pub(super) fn render_details(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = v_flex()
            .id("details")
            .w(px(DETAILS_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar);

        let Some(package) = self.selected.and_then(|ix| self.package(ix)) else {
            return panel
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Select a package")
                .into_any_element();
        };

        let ix = self.selected.unwrap_or_default();
        let update = self.updates.get(package);
        let update_button = update.map(|_| {
            let (label, tooltip) = match &package.source {
                Source::Repo(_) => ("Update System", "Repo packages update together: sudo pacman -Syu".to_string()),
                Source::Aur => match self.aur_helper() {
                    Some(helper) => ("Update", format!("Rebuild with {}", helper.program())),
                    None => ("Update", "Install paru or yay to update AUR packages".to_string()),
                },
                Source::Flatpak { .. } => ("Update", "flatpak update".to_string()),
            };
            Button::new("update-package")
                .primary()
                .small()
                .icon(Icon::new(CatalogIcon::CircleArrowUp))
                .label(label)
                .tooltip(tooltip)
                .disabled(package.source == Source::Aur && self.aur_helper().is_none())
                .on_click(cx.listener(move |this, _, window, cx| this.update_package(ix, window, cx)))
        });
        panel
            .p_5()
            .gap_5()
            .overflow_y_scrollbar()
            .child(header(package, cx))
            .child(
                h_flex().gap_2().children(update_button).child(
                    Button::new("remove-package")
                        .danger()
                        .outline()
                        .small()
                        .icon(IconName::Delete)
                        .label("Remove")
                        .tooltip("Preview what gets removed, then remove it")
                        .on_click(cx.listener(move |this, _, window, cx| this.remove_package(ix, window, cx))),
                ),
            )
            .child(facts(package, update, cx))
            .children(self.chip_section("Depends on", &package.depends, cx))
            .children(self.chip_section("Required by", &package.required_by, cx))
            .children(self.chip_section("Optional for", &package.optional_for, cx))
            .children(optional_section(&package.optional_depends, cx))
            .into_any_element()
    }

    /// A titled list of package names. Each one that's installed jumps to it.
    fn chip_section(&self, title: &str, names: &[String], cx: &mut Context<Self>) -> Option<AnyElement> {
        if names.is_empty() {
            return None;
        }
        let theme = cx.theme();
        let extra = names.len().saturating_sub(MAX_CHIPS);
        let chips: Vec<AnyElement> = names
            .iter()
            .take(MAX_CHIPS)
            .enumerate()
            .map(|(ix, name)| {
                let target = strip_constraint(name).to_string();
                div()
                    .id(SharedString::from(format!("{title}-{ix}")))
                    .px_2()
                    .py_0p5()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.list_hover))
                    .child(name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| this.reveal(&target, window, cx)))
                    .into_any_element()
            })
            .collect();

        Some(
            v_flex()
                .gap_2()
                .child(section_title(title, names.len(), cx))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(chips)
                        .when(extra > 0, |this| {
                            this.child(
                                div()
                                    .px_1()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format!("and {extra} more")),
                            )
                        }),
                )
                .into_any_element(),
        )
    }
}

fn header(package: &Package, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let name = package.display_name().to_string();
    let reason = match (package.orphan, package.reason) {
        (true, _) => Some(Tag::danger().xsmall().outline().child("Orphan")),
        (false, Reason::Dependency) => Some(Tag::secondary().xsmall().outline().child("Dependency")),
        (false, Reason::Explicit) => None,
    };

    v_flex()
        .gap_2()
        .child(div().text_lg().font_semibold().child(name.clone()))
        .when(name != package.name, |this| {
            this.child(div().text_xs().text_color(theme.muted_foreground).child(package.name.clone()))
        })
        .child(
            h_flex()
                .gap_1()
                .flex_wrap()
                .child(source_tag(&package.source))
                .children(reason),
        )
        .when(!package.description.is_empty(), |this| {
            this.child(div().text_sm().text_color(theme.muted_foreground).child(package.description.clone()))
        })
}

fn facts(package: &Package, update: Option<&str>, cx: &App) -> impl IntoElement {
    let mut rows: Vec<(&str, String)> = vec![("Version", package.version.clone())];
    if let Some(new) = update {
        rows.push(("Update", new.to_string()));
    }
    match &package.source {
        Source::Repo(repo) => rows.push(("Repository", repo.clone())),
        Source::Aur => rows.push(("Repository", "AUR or local build".into())),
        Source::Flatpak { installation } => rows.push(("Installation", installation.clone())),
    }
    rows.push(("Size", format_size(package.size)));
    if let Some(date) = package.install_date {
        rows.push(("Installed", format_date(date)));
    }
    if !package.licenses.is_empty() {
        rows.push(("License", package.licenses.join(", ")));
    }

    let theme = cx.theme();
    v_flex()
        .gap_1p5()
        .children(rows.into_iter().map(|(label, value)| fact_row(label, value.into_any_element(), cx)))
        .when_some(package.url.clone(), |this, url| {
            let link = div()
                .id("homepage")
                .truncate()
                .text_color(theme.link)
                .cursor_pointer()
                .hover(|style| style.text_color(theme.link_hover))
                .child(url.clone())
                .on_click(move |_, _, cx| cx.open_url(&url))
                .into_any_element();
            this.child(fact_row("Website", link, cx))
        })
}

fn fact_row(label: &str, value: AnyElement, cx: &App) -> AnyElement {
    h_flex()
        .gap_3()
        .text_sm()
        .items_start()
        .child(
            div()
                .w(px(92.))
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(div().flex_1().min_w_0().child(value))
        .into_any_element()
}

/// Optional dependencies, as `name: why`, with installed ones marked.
fn optional_section(lines: &[String], cx: &App) -> Option<AnyElement> {
    if lines.is_empty() {
        return None;
    }
    let theme = cx.theme();
    Some(
        v_flex()
            .gap_2()
            .child(section_title("Optional dependencies", lines.len(), cx))
            .children(lines.iter().map(|line| {
                let (name, why) = line.split_once(": ").unwrap_or((line.as_str(), ""));
                v_flex()
                    .text_xs()
                    .child(div().font_medium().child(name.to_string()))
                    .when(!why.is_empty(), |this| {
                        this.child(div().text_color(theme.muted_foreground).child(why.to_string()))
                    })
            }))
            .into_any_element(),
    )
}

fn section_title(title: &str, count: usize, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_2()
        .items_baseline()
        .child(div().text_sm().font_medium().child(title.to_string()))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(count.to_string()))
}

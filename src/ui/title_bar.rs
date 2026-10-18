use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, TitleBar, h_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::*;

use super::{KEY_CONTEXT, Load, Packrat, Refresh};

/// Title bar height; gpui-kit's default is 34px.
const TITLE_BAR_HEIGHT: f32 = 44.;
const SEARCH_WIDTH: f32 = 420.;

impl Packrat {
    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let loading = matches!(self.load, Load::Loading);

        TitleBar::new().h(px(TITLE_BAR_HEIGHT)).child(
            h_flex()
                .w_full()
                .gap_3()
                .items_center()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .flex_shrink_0()
                        .child(Icon::new(CatalogIcon::Package).size_4().text_color(cx.theme().muted_foreground))
                        .child(div().text_sm().font_semibold().child("Packrat")),
                )
                .child(
                    div().flex_1().min_w_0().flex().justify_center().child(
                        div().w(px(SEARCH_WIDTH)).max_w_full().child(
                            Input::new(&self.search)
                                .small()
                                .cleanable(true)
                                .prefix(Icon::new(IconName::Search).small().text_color(cx.theme().muted_foreground)),
                        ),
                    ),
                )
                .child(
                    Button::new("settings")
                        .ghost()
                        .small()
                        .icon(Icon::new(CatalogIcon::Settings))
                        .accessibility_label("Settings")
                        .tooltip("Settings")
                        .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
                )
                .child(
                    Button::new("refresh")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .loading(loading)
                        .accessibility_label("Reload Packages")
                        .tooltip_with_action("Reload Packages", &Refresh, Some(KEY_CONTEXT))
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(Refresh), cx)),
                ),
        )
    }
}

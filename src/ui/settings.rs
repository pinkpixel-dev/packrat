use gpui_kit::component::button::Button;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::*;

use super::Packrat;
use crate::settings::AurHelper;

const DIALOG_WIDTH: f32 = 380.;

impl Packrat {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let this = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let (installed, current) = this
                .upgrade()
                .map(|packrat| {
                    let packrat = packrat.read(cx);
                    (packrat.aur_helpers.clone(), packrat.aur_helper())
                })
                .unwrap_or_default();
            let radios = AurHelper::ALL.map(|helper| {
                let available = installed.contains(&helper);
                let label = if available {
                    helper.program().to_string()
                } else {
                    format!("{} (not installed)", helper.program())
                };
                Radio::new(helper.program()).label(label).disabled(!available)
            });
            let selected = current.and_then(|current| AurHelper::ALL.iter().position(|&helper| helper == current));
            let this = this.clone();

            dialog
                .w(px(DIALOG_WIDTH))
                .title("Settings")
                .child(
                    v_flex().gap_2().child(div().text_sm().font_medium().child("AUR helper")).child(
                        RadioGroup::horizontal("aur-helper")
                            .children(radios)
                            .selected_index(selected)
                            .on_click(move |&ix, window, cx| {
                                _ = this.update(cx, |this, cx| this.set_aur_helper(AurHelper::ALL[ix], window, cx));
                            }),
                    ),
                )
                .footer(
                    h_flex().w_full().justify_end().child(
                        Button::new("settings-close")
                            .outline()
                            .label("Close")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
                )
        });
    }

    fn set_aur_helper(&mut self, helper: AurHelper, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.aur_helper = Some(helper);
        if let Err(err) = self.settings.save() {
            window.push_notification(Notification::error(format!("Couldn't save settings: {err}")), cx);
        }
        cx.notify();
    }
}

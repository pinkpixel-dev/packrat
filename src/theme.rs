use gpui_kit::component::{Theme, ThemeSet};
use gpui_kit::*;

const BUNDLED: &str = include_str!("../themes/packrat.json");

/// Applies Packrat's dark theme.
pub fn init(cx: &mut App) {
    let set: ThemeSet = match serde_json::from_str(BUNDLED) {
        Ok(set) => set,
        Err(err) => {
            eprintln!("packrat: the bundled theme failed to parse: {err}");
            return;
        }
    };
    let Some(config) = set.themes.into_iter().next() else {
        return;
    };
    Theme::update(cx, |theme| theme.apply_config(&config.into()));
}

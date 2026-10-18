mod assets;
mod model;
mod ops;
mod settings;
mod sources;
mod terminal;
mod theme;
mod ui;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

use crate::ui::Packrat;

const APP_ID: &str = "dev.pinkpixel.Packrat";

fn main() {
    application().with_assets(assets::AppAssets).run(|cx| {
        init(cx);
        theme::init(cx);
        terminal::init(cx);
        ui::init(cx);

        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(720.), px(420.))),
            window_decorations: Some(WindowDecorations::Client),
            app_id: Some(APP_ID.into()),
            ..TitleBar::window_options()
        };

        if let Err(err) = open_window(options, cx, |window, cx| cx.new(|cx| Packrat::new(window, cx))) {
            eprintln!("{}", window_error_message(&err.to_string()));
            // Nothing has opened yet; `cx.quit()` here would leave GPUI's loop running.
            std::process::exit(1);
        }
        cx.activate(true);
    });
}

/// What to print when the window can't open. Almost always this means no
/// usable Vulkan driver, so point at the usual causes instead of a backtrace.
fn window_error_message(err: &str) -> String {
    format!(
        "packrat: couldn't open a window: {err}\n\n\
         Packrat draws with Vulkan, so this usually means no Vulkan driver is available.\n\
         Run `vulkaninfo --summary` to see what your system has, and install the driver\n\
         for your GPU (vulkan-radeon, vulkan-intel, or nvidia-utils on Arch)."
    )
}

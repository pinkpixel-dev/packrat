use std::borrow::Cow;

use gpui_kit::assets::{Assets as ComponentAssets, icon_assets};
use gpui_kit::{AssetSource, Result, SharedString};

// Lucide icons Packrat uses beyond Kit's default component set.
icon_assets!(
    ExtraIcons,
    [
        Code,
        Image,
        Clapperboard,
        Gamepad2,
        GraduationCap,
        FlaskConical,
        MonitorCog,
        Wrench,
        SlidersHorizontal,
        AppWindow,
        Terminal,
        Server,
        Blocks,
        Braces,
        Type,
        Paintbrush,
        Boxes,
        Shapes,
        Package,
        LayoutGrid,
        Pin,
        Unlink,
        ArrowDownUp,
        CircleArrowUp,
        Settings,
    ]
);

/// Kit's default icons plus Packrat's extras.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // Check the extras first: the default source errors on unknown paths.
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        ComponentAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = ComponentAssets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

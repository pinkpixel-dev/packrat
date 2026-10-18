pub mod flatpak;
pub mod pacman;
pub mod updates;

use std::path::Path;

use crate::model::inventory::Inventory;

/// Reads everything installed. Blocking: run it off the UI thread.
pub fn load_inventory() -> Result<Inventory, String> {
    let repos = pacman::sync_repos();
    let mut packages = pacman::load(Path::new(pacman::LOCAL_DB), &pacman::root(), &repos)
        .map_err(|err| format!("Couldn't read the pacman database at {}: {err}", pacman::LOCAL_DB))?;
    packages.extend(flatpak::load());
    Ok(Inventory::new(packages))
}

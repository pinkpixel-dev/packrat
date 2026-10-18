# Changelog

## 0.3.0 - Unreleased

### ⬆️ Updates

- New **Updates** view in the sidebar, with a count of everything that has a newer version waiting
- Checks repo packages with `checkupdates`, AUR packages with `paru -Qua` or `yay -Qua`, and Flatpaks remote by remote
- Checks once at startup, then again whenever you click **Check**
- Rows show `old → new` versions in every view, and the details panel shows the new version
- **Update All** runs `sudo pacman -Syu`, then the AUR helper, then `flatpak update` for each installation, in the terminal pane
- **Update** in the details panel rebuilds one AUR package or updates one Flatpak
- Repo packages get **Update System** instead, since updating one by itself would be a partial upgrade
- Full upgrades keep the pane open afterwards so you can read pacman's notes

### ⚙️ Settings

- New settings dialog (gear in the title bar) to pick paru or yay as your AUR helper
- Saved to `~/.config/packrat/settings.json`
- Without a choice, Packrat uses paru, or yay when paru isn't installed

## 0.2.0 - October 7, 2026

### 🖥️ Terminal pane

- New built-in terminal at the bottom of the window, using alacritty_terminal with a real PTY
- Colors, scrollback, mouse selection, `Ctrl+Shift+C` / `Ctrl+Shift+V`, and a resizable pane
- `` Ctrl+` `` opens or hides it, and **Open Shell** starts your own shell in it
- Tab, `Ctrl+R`, `Ctrl+F` and `Ctrl+L` go to the program in the pane while it has focus
- Finished programs show Done or Failed in the header; `Enter` or `Esc` closes the pane

### 🗑️ Removing packages

- **Remove** button for repo, AUR, and Flatpak packages
- Preview dialog from `pacman -Rsp` listing everything that would go, the space freed, and the exact command
- Removals run as `sudo pacman -Rns` in the pane, so pacman's own `[Y/n]` still applies
- Shows pacman's reason when a package can't be removed, without running anything
- **Remove All** in the Orphans view
- System Flatpaks uninstall through `sudo`, so they work without a polkit agent
- **Remove Unused** in Flatpak Runtimes
- The list reloads after a successful command, and the pane closes itself a moment later

### 🏷️ Versioning

- Bumped to 0.2.0

## 0.1.0 - October 7, 2026

### 📦 Package browser

- First version: browse every installed pacman package, AUR package, and Flatpak in one list
- Sidebar views for Apps, All Packages, Installed by You, and Orphans
- 23 categories worked out from launchers, package names, and installed files
- Source filter (Repos, AUR, Flatpak) and sorting by name, size, or install date
- Live search across package names, app names, and descriptions
- Details panel with version, size, install date, license, website, dependencies, reverse dependencies, and optional dependencies
- Click a dependency to jump to it
- Keyboard navigation for the list, search, and sidebar views

### 🎨 Look

- Packrat Dark theme with a custom title bar

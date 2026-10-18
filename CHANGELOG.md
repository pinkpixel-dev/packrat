# Changelog

## 1.0.0 - October 18, 2026

### 🔍 Find and install

- New **Find** view at the top of the sidebar. While it's open, the search box looks for packages you don't have yet
- Searches the repos (`pacman -Ss`), the AUR (official RPC API), and every Flatpak remote (`flatpak search`) at the same time
- Results are ranked by name match first, so "obsidian" puts the Obsidian app ahead of GTK themes with the same name
- Searches with several words match each word separately everywhere, and dashes count as spaces, so "muse code" finds `muse-code-bin`
- Already installed results get an **Installed** tag and a **Show in Library** button
- AUR results show votes, popularity, maintainer, and an out-of-date tag
- **Install** runs `sudo pacman -S`, `paru -S --aur` / `yay -S --aur`, or `flatpak install` in the terminal pane
- If repo updates are waiting, repo installs become `sudo pacman -Syu <package>` so they never install against an old database
- Flatpaks install system-wide when the remote is set up there
- If the AUR or Flatpak can't be searched, a short note under the header says so, and the other results still show up

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

### 🎨 Branding

- New Packrat logo in the title bar, replacing the generic box icon
- App icon and `.desktop` file in `packaging/`

### 📦 Packaging

- AUR PKGBUILD in `packaging/aur/` that builds from the release tag, runs the tests, and installs the binary, desktop entry, icon, and license
- `pacman-contrib`, `paru`, `yay`, and `flatpak` are optional dependencies, since Packrat works without them

### 🏷️ Versioning

- Bumped to 1.0.0, the first release published to the AUR

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

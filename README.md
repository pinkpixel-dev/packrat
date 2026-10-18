# Packrat

Packrat is a desktop app for browsing everything installed on an Arch Linux system: repo packages, AUR packages, and Flatpaks, all in one list.

I wanted an easier way to see what's actually on my machine. `pacman -Q` gives you about two thousand lines with no real way to tell an app from a library, so Packrat sorts everything into categories you can click through, with search and a details panel for each package.

You can also remove packages from it. Anything that changes your system runs as a real command in a terminal pane at the bottom of the window, so you always see exactly what's running and answer the password and `[Y/n]` prompts yourself. You can update from it too. Installing new packages isn't in yet.

## What it does

- Lists every installed pacman package and Flatpak (apps and runtimes)
- Marks where each one came from: the repo name, AUR, or Flatpak
- Sorts packages into categories like Development, Games, Command Line, Libraries, Fonts, and Kernel & Drivers
- Shows **Apps** (anything with a launcher), **Installed by You**, and **Orphans** (leftover dependencies nothing needs anymore)
- Filters by source and sorts by name, size, or install date
- Searches names, app names, and descriptions as you type
- Shows each package's version, size, install date, license, website, dependencies, what needs it, and optional dependencies
- Click any dependency to jump straight to it
- Remove a package (repo, AUR, or Flatpak), or clear out every orphan at once
- See pending updates for repo packages, AUR packages, and Flatpaks, and update them
- A built-in terminal pane, which you can also open as a normal shell

## How it figures out categories

Arch doesn't tag packages by type, so Packrat works it out:

1. If a package ships a desktop launcher, it's an app, and the launcher's own category (Network, Graphics, Game, and so on) decides where it goes.
2. Otherwise the name and installed files decide. `python-*` goes to Python, `ttf-*` to Fonts, anything with kernel modules or firmware to Kernel & Drivers, `lib*` to Libraries, packages with systemd services to Services, and anything else that installs to `/usr/bin` to Command Line.
3. What's left (configs, keyrings, mirror lists, meta packages) lands in Config & Data.

It's a heuristic, so a few packages will end up somewhere slightly odd. Search covers those.

## Removing packages

Select a package and click **Remove** in the details panel. Before anything runs, Packrat asks pacman for a dry run (`pacman -Rsp`, no root needed) and shows you every package that would go, including dependencies nothing else needs, plus how much space you'd get back and the exact command it's about to run.

Click **Remove** in that dialog and the command runs in the terminal pane:

```bash
sudo pacman -Rns <package>
```

You type your password in the pane, and pacman still asks its own `[Y/n]`, so there's one last chance to back out. If pacman won't remove something (because other packages need it), you get its reason instead and nothing runs.

In the **Orphans** view, **Remove All** does the same thing for every orphan in one go.

In **Flatpak Runtimes**, **Remove Unused** runs `flatpak uninstall --unused` for your user and system installations. Flatpak lists what it's about to remove and asks before doing anything, so this one goes straight to the pane.

Flatpaks use `flatpak uninstall`. System-wide Flatpaks get `sudo` in front, which keeps them working on setups without a polkit agent (Niri, for example).

When a command finishes successfully, the package list reloads and the pane closes itself after a moment. If something fails, the pane stays open so you can read the error.

## Updating

Packrat checks for updates once when it starts. The **Updates** view in the sidebar lists everything with a newer version waiting, and every row shows the version change (`1.0.8 → 1.0.9`). Click **Check** in that view to look again.

The checks don't touch your real package databases:

- Repo packages come from `checkupdates` (part of `pacman-contrib`), which syncs a throwaway copy of the databases
- AUR packages come from `paru -Qua` or `yay -Qua`
- Flatpaks come from `flatpak remote-ls --updates`, one remote at a time, so one broken remote doesn't hide the rest

**Update All** runs everything in the terminal pane, one step after another, stopping if a step fails:

```bash
sudo pacman -Syu && paru -Sua && flatpak update --user && sudo flatpak update --system
```

Packrat only includes the steps that have something to update. The pane stays open afterwards, because pacman sometimes prints things worth reading (like new `.pacnew` files).

You can also update a single package from the details panel. AUR packages get rebuilt with `paru -S --aur <package>` and Flatpaks run `flatpak update <id>`. Repo packages show **Update System** instead, which runs a full `sudo pacman -Syu`. Updating one repo package by itself is a partial upgrade, which Arch doesn't support and which can break things in fun ways.

### Picking paru or yay

Click the gear in the title bar to choose your AUR helper. Packrat saves the choice to `~/.config/packrat/settings.json`. If you never pick one, it uses paru, or yay when paru isn't installed.

## The terminal pane

Press `` Ctrl+` `` to open or hide it, and drag its top edge to resize it. With nothing running, **Open Shell** starts your normal shell (`$SHELL`) in it.

It's a real terminal, using the same emulator as Alacritty, so colors, progress bars, and tab completion all work. Select text with the mouse, copy with `Ctrl+Shift+C`, and paste with `Ctrl+Shift+V`. After a program exits, press `Enter` or `Esc` to close the pane, or click **New Shell**.

## Requirements

- Arch Linux or an Arch-based distro (I use CachyOS)
- A GPU with a working Vulkan driver, since GPUI draws with Vulkan
- `sudo` set up for your user, since removals and updates run `sudo pacman`
- `pacman-contrib` for repo update checks (`checkupdates`)
- `paru` or `yay` if you want AUR updates
- Flatpak is optional. Without it you just won't see a Flatpak section.

## Building

You need Rust 1.92 or newer.

```bash
git clone https://github.com/pinkpixel-dev/packrat.git
cd packrat
cargo run --release
```

## Keyboard

| Key | What it does |
|---|---|
| `Ctrl+F` | Jump to search |
| `/` | Jump to search (from the list) |
| `Enter` | From search, move to the list |
| `Ctrl+L` | Focus the list |
| `↑` `↓` or `k` `j` | Move through the list |
| `Home` `End` | First or last package |
| `Alt+↑` `Alt+↓` | Previous or next sidebar view |
| `` Ctrl+` `` | Open or hide the terminal pane |
| `Ctrl+R` or `F5` | Reload packages |
| `Ctrl+Q` | Quit |

While the terminal pane has focus, `Tab`, `Ctrl+R`, `Ctrl+F` and `Ctrl+L` go to the program running in it instead of Packrat.

## How it reads your packages

Packrat reads pacman's local database files in `/var/lib/pacman/local` directly instead of linking libalpm. That keeps it working when pacman bumps its library version, and it means Packrat never needs root just to show you what's installed. Root only comes in through `sudo` in the terminal pane, for commands you confirm. Repo names come from `pacman -Sl`, which only reads the sync databases already on disk, and Flatpaks come from `flatpak list`.

AUR and orphan detection match what `pacman -Qm` and `pacman -Qdt` report.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)

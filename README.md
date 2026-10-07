# Packrat

Packrat is a desktop app for browsing everything installed on an Arch Linux system: repo packages, AUR packages, and Flatpaks, all in one list.

I wanted an easier way to see what's actually on my machine. `pacman -Q` gives you about two thousand lines with no real way to tell an app from a library, so Packrat sorts everything into categories you can click through, with search and a details panel for each package.

Right now Packrat is read-only. It shows you what's installed, but it doesn't install, update, or remove anything yet. Removing packages, updates, and installing new ones are coming next, all through a built-in terminal pane so you can see exactly what runs.

## What it does

- Lists every installed pacman package and Flatpak (apps and runtimes)
- Marks where each one came from: the repo name, AUR, or Flatpak
- Sorts packages into categories like Development, Games, Command Line, Libraries, Fonts, and Kernel & Drivers
- Shows **Apps** (anything with a launcher), **Installed by You**, and **Orphans** (leftover dependencies nothing needs anymore)
- Filters by source and sorts by name, size, or install date
- Searches names, app names, and descriptions as you type
- Shows each package's version, size, install date, license, website, dependencies, what needs it, and optional dependencies
- Click any dependency to jump straight to it

## How it figures out categories

Arch doesn't tag packages by type, so Packrat works it out:

1. If a package ships a desktop launcher, it's an app, and the launcher's own category (Network, Graphics, Game, and so on) decides where it goes.
2. Otherwise the name and installed files decide. `python-*` goes to Python, `ttf-*` to Fonts, anything with kernel modules or firmware to Kernel & Drivers, `lib*` to Libraries, packages with systemd services to Services, and anything else that installs to `/usr/bin` to Command Line.
3. What's left (configs, keyrings, mirror lists, meta packages) lands in Config & Data.

It's a heuristic, so a few packages will end up somewhere slightly odd. Search covers those.

## Requirements

- Arch Linux or an Arch-based distro (I use CachyOS)
- A GPU with a working Vulkan driver, since GPUI draws with Vulkan
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
| `Ctrl+R` or `F5` | Reload packages |
| `Ctrl+Q` | Quit |

## How it reads your packages

Packrat reads pacman's local database files in `/var/lib/pacman/local` directly instead of linking libalpm. That keeps it working when pacman bumps its library version, and it means Packrat never needs root just to show you what's installed. Repo names come from `pacman -Sl`, which only reads the sync databases already on disk, and Flatpaks come from `flatpak list`.

AUR and orphan detection match what `pacman -Qm` and `pacman -Qdt` report.

## License

Apache 2.0. See [LICENSE](LICENSE).

Made with 💖 by [Pink Pixel](https://pinkpixel.dev)

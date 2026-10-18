//! Pending updates, and parsing what the update checkers print.

use std::collections::HashMap;

use super::package::{Package, Source};

/// Which installed thing an update is for. pacman names are unique across
/// repo and AUR packages; Flatpak IDs repeat across installations.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Pacman(String),
    Flatpak { id: String, installation: String },
}

impl Key {
    pub fn of(package: &Package) -> Self {
        match &package.source {
            Source::Flatpak { installation } => Key::Flatpak {
                id: package.name.clone(),
                installation: installation.clone(),
            },
            Source::Repo(_) | Source::Aur => Key::Pacman(package.name.clone()),
        }
    }
}

/// Every pending update, with the version it would bring.
#[derive(Clone, Debug, Default)]
pub struct Updates(HashMap<Key, String>);

impl Updates {
    pub fn insert(&mut self, key: Key, version: String) {
        self.0.insert(key, version);
    }

    /// The new version, if `package` has one waiting.
    pub fn get(&self, package: &Package) -> Option<&str> {
        self.0.get(&Key::of(package)).map(String::as_str)
    }

    pub fn remove(&mut self, key: &Key) {
        self.0.remove(key);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Parses `name old -> new` lines from `checkupdates`, `paru -Qua` and
/// `yay -Qua` (yay adds ` [age]` at the end). Returns names and new versions.
pub fn parse_pacman(stdout: &str) -> Vec<(String, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let _old = fields.next()?;
            if fields.next()? != "->" {
                return None;
            }
            Some((name.to_string(), fields.next()?.to_string()))
        })
        .collect()
}

/// Parses `flatpak remote-ls --updates --columns=application,version,branch`.
/// Runtimes often have no version, so the branch stands in, the same way the
/// installed list shows them.
pub fn parse_flatpak(stdout: &str, installation: &str) -> Vec<(Key, String)> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t').map(str::trim);
            let id = fields.next().filter(|id| !id.is_empty())?;
            let version = fields.next().unwrap_or_default();
            let branch = fields.next().unwrap_or_default();
            let version = if version.is_empty() { branch } else { version };
            let key = Key::Flatpak {
                id: id.to_string(),
                installation: installation.to_string(),
            };
            Some((key, version.to_string()))
        })
        .collect()
}

/// A Flatpak remote from `flatpak remotes --columns=name,options`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    /// `user`, `system`, or a custom installation's name.
    pub installation: String,
    /// Local remotes from flatpak-builder are `no-enumerate` and usually
    /// can't be listed, so their errors aren't worth showing.
    pub enumerable: bool,
}

pub fn parse_remotes(stdout: &str) -> Vec<Remote> {
    stdout
        .lines()
        .filter_map(|line| {
            let (name, options) = line.split_once('\t')?;
            let options: Vec<&str> = options.trim().split(',').collect();
            if options.contains(&"disabled") {
                return None;
            }
            Some(Remote {
                name: name.trim().to_string(),
                installation: options.first()?.to_string(),
                enumerable: !options.contains(&"no-enumerate"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_checkupdates_and_helpers() {
        let parsed = parse_pacman(
            "avahi 1:0.9rc5-1.1 -> 1:0.9rc5-2.1\n\
             docker-desktop 4.93.0-1 -> 4.95.0-1 [6d]\n\
             :: Searching AUR for updates...\n\n",
        );
        assert_eq!(
            parsed,
            [
                ("avahi".to_string(), "1:0.9rc5-2.1".to_string()),
                ("docker-desktop".to_string(), "4.95.0-1".to_string()),
            ]
        );
    }

    #[test]
    fn parses_flatpak_updates() {
        let parsed = parse_flatpak(
            "com.discordapp.Discord\t1.0.162\tstable\norg.gnome.Platform\t\t50\n",
            "system",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].1, "1.0.162");
        assert_eq!(
            parsed[1],
            (
                Key::Flatpak { id: "org.gnome.Platform".into(), installation: "system".into() },
                "50".into()
            )
        );
    }

    #[test]
    fn parses_remotes() {
        let remotes = parse_remotes(
            "flathub\tsystem\n\
             debug-origin\tuser,no-enumerate,no-gpg-verify\n\
             old\tuser,disabled\n",
        );
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0], Remote { name: "flathub".into(), installation: "system".into(), enumerable: true });
        assert_eq!(remotes[1].installation, "user");
        assert!(!remotes[1].enumerable);
    }
}

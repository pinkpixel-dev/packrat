//! Installed pacman packages, read straight from the local database
//! (`/var/lib/pacman/local/<name>-<version>/desc` and `files`).
//!
//! Reading the files instead of linking libalpm keeps Packrat working across
//! libalpm version bumps and keeps it out of the alpm crate's GPL.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::model::category::classify;
use crate::model::launcher;
use crate::model::package::{Package, Reason, Source};

pub const LOCAL_DB: &str = "/var/lib/pacman/local";

/// Loads every installed pacman package. `repos` maps package names to the
/// sync repo that has them (see [`sync_repos`]); anything missing from it is
/// treated as AUR.
pub fn load(db: &Path, root: &Path, repos: &HashMap<String, String>) -> std::io::Result<Vec<Package>> {
    let mut packages = Vec::new();
    for entry in std::fs::read_dir(db)? {
        let dir = entry?.path();
        let Ok(desc) = std::fs::read_to_string(dir.join("desc")) else {
            continue;
        };
        let files = std::fs::read_to_string(dir.join("files")).unwrap_or_default();
        if let Some(package) = parse_package(&desc, &files, root, repos) {
            packages.push(package);
        }
    }
    Ok(packages)
}

/// Asks pacman which repo carries each package, from the sync databases on
/// disk. No network: this reads what the last `pacman -Sy` downloaded.
pub fn sync_repos() -> HashMap<String, String> {
    let Ok(output) = Command::new("pacman").arg("-Sl").output() else {
        return HashMap::new();
    };
    parse_sync_list(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `pacman -Sl` lines (`repo name version [installed]`). When two repos
/// carry the same name, the first one listed wins, like pacman's own order.
pub fn parse_sync_list(output: &str) -> HashMap<String, String> {
    let mut repos = HashMap::new();
    for line in output.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(repo), Some(name)) = (fields.next(), fields.next()) {
            repos.entry(name.to_string()).or_insert_with(|| repo.to_string());
        }
    }
    repos
}

/// Splits a `desc` file into its `%KEY%` sections.
fn sections(desc: &str) -> HashMap<&str, Vec<&str>> {
    let mut sections: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut current = None;
    for line in desc.lines() {
        if line.len() > 2 && line.starts_with('%') && line.ends_with('%') {
            current = Some(&line[1..line.len() - 1]);
            sections.entry(current.unwrap()).or_default();
        } else if !line.is_empty()
            && let Some(key) = current
        {
            sections.entry(key).or_default().push(line);
        }
    }
    sections
}

pub fn parse_package(desc: &str, files: &str, root: &Path, repos: &HashMap<String, String>) -> Option<Package> {
    let sections = sections(desc);
    let first = |key: &str| sections.get(key).and_then(|values| values.first()).map(|value| value.to_string());
    let all = |key: &str| {
        sections
            .get(key)
            .map(|values| values.iter().map(|value| value.to_string()).collect::<Vec<_>>())
            .unwrap_or_default()
    };

    let name = first("NAME")?;
    let files: Vec<String> = files
        .lines()
        .skip_while(|line| *line != "%FILES%")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .map(String::from)
        .collect();

    // A package can ship several launchers (settings panels, helpers); the
    // first visible one stands for the package.
    let launcher = files
        .iter()
        .filter(|file| file.starts_with("usr/share/applications/") && file.ends_with(".desktop"))
        .find_map(|file| launcher::read(&root.join(file)));

    // Like `pacman -Qm`, anything no configured repo carries anymore counts as
    // foreign. pacman 7.1+ also records which repo it came from, which is more
    // precise when two repos carry the same name.
    let source = match repos.get(&name) {
        Some(repo) => Source::Repo(first("INSTALLED_DB").unwrap_or_else(|| repo.clone())),
        None => Source::Aur,
    };
    let reason = match first("REASON").as_deref() {
        Some("1") => Reason::Dependency,
        _ => Reason::Explicit,
    };
    let category = classify(&name, &files, launcher.as_ref().map(|launcher| launcher.categories.as_slice()));

    Some(Package {
        version: first("VERSION").unwrap_or_default(),
        description: first("DESC").unwrap_or_default(),
        source,
        reason,
        size: first("SIZE").and_then(|size| size.parse().ok()).unwrap_or(0),
        install_date: first("INSTALLDATE").and_then(|date| date.parse().ok()),
        url: first("URL"),
        licenses: all("LICENSE"),
        depends: all("DEPENDS"),
        optional_depends: all("OPTDEPENDS"),
        required_by: Vec::new(),
        optional_for: Vec::new(),
        provides: all("PROVIDES"),
        launcher,
        category,
        orphan: false,
        name,
    })
}

/// The filesystem root package paths are relative to.
pub fn root() -> PathBuf {
    PathBuf::from("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DESC: &str = "%NAME%\nripgrep\n\n%VERSION%\n15.1.0-1\n\n%DESC%\nA search tool\n\n\
        %URL%\nhttps://github.com/BurntSushi/ripgrep\n\n%SIZE%\n6000000\n\n%REASON%\n1\n\n\
        %LICENSE%\nMIT\nUnlicense\n\n%DEPENDS%\ngcc-libs\nglibc>=2.38\npcre2\n\n\
        %OPTDEPENDS%\nbash-completion: completions\n\n%PROVIDES%\nrg=15.1.0\n";
    const FILES: &str = "%FILES%\nusr/\nusr/bin/\nusr/bin/rg\n\n%BACKUP%\n";

    #[test]
    fn parses_a_desc_file() {
        let repos = HashMap::from([("ripgrep".to_string(), "extra".to_string())]);
        let package = parse_package(DESC, FILES, Path::new("/nonexistent"), &repos).unwrap();
        assert_eq!(package.name, "ripgrep");
        assert_eq!(package.version, "15.1.0-1");
        assert_eq!(package.source, Source::Repo("extra".into()));
        assert_eq!(package.reason, Reason::Dependency);
        assert_eq!(package.size, 6_000_000);
        assert_eq!(package.licenses, ["MIT", "Unlicense"]);
        assert_eq!(package.depends, ["gcc-libs", "glibc>=2.38", "pcre2"]);
        assert_eq!(package.optional_depends, ["bash-completion: completions"]);
        assert_eq!(package.provides, ["rg=15.1.0"]);
        assert_eq!(package.category, crate::model::category::Category::CommandLine);
    }

    #[test]
    fn missing_from_repos_means_aur_and_installed_db_wins() {
        let package = parse_package(DESC, FILES, Path::new("/nonexistent"), &HashMap::new()).unwrap();
        assert_eq!(package.source, Source::Aur);

        let repos = HashMap::from([("ripgrep".to_string(), "extra".to_string())]);
        let desc = format!("{DESC}\n%INSTALLED_DB%\ncachyos-extra-v3\n");
        let package = parse_package(&desc, FILES, Path::new("/nonexistent"), &repos).unwrap();
        assert_eq!(package.source, Source::Repo("cachyos-extra-v3".into()));

        // Installed from a repo that has since dropped it: foreign now.
        let package = parse_package(&desc, FILES, Path::new("/nonexistent"), &HashMap::new()).unwrap();
        assert_eq!(package.source, Source::Aur);
    }

    #[test]
    fn sync_list_keeps_the_first_repo() {
        let repos = parse_sync_list("cachyos-v3 bash 5.3-2 [installed]\ncore bash 5.3-1\nextra vim 9.1-1\n");
        assert_eq!(repos["bash"], "cachyos-v3");
        assert_eq!(repos["vim"], "extra");
    }
}

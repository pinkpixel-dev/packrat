//! Commands that change the system, and the dry runs that preview them.
//!
//! Everything here builds a [`Command`] for the terminal pane. Nothing runs
//! with root access on its own: pacman goes through `sudo` inside the pane,
//! so the password prompt and pacman's own `[Y/n]` stay in front of you.

use std::process::Command as Process;

use crate::model::package::{Package, Source};
use crate::settings::AurHelper;
use crate::sources::updates::installation_flag;
use crate::terminal::Command;

/// One package a removal would take out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub version: String,
    pub size: u64,
}

/// What a removal would do, worked out before anything runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preview {
    /// Everything that goes, including dependencies nothing else needs.
    Removes(Vec<Target>),
    /// pacman refused, usually because other packages still need it.
    Blocked(String),
}

impl Preview {
    pub fn total_size(targets: &[Target]) -> u64 {
        targets.iter().map(|target| target.size).sum()
    }
}

/// Asks pacman what `pacman -Rns <names>` would remove, without root. `-n`
/// can't be combined with `--print`, but it only changes which config files
/// get backed up, not which packages go.
pub fn preview_pacman_removal(names: &[String]) -> Preview {
    let output = Process::new("pacman")
        .args(["-Rsp", "--print-format", "%n %v %s"])
        .args(names)
        .env("LC_ALL", "C")
        .output();
    match output {
        Ok(output) if output.status.success() => Preview::Removes(parse_print(&String::from_utf8_lossy(&output.stdout))),
        Ok(output) => Preview::Blocked(blocked_reason(&String::from_utf8_lossy(&output.stderr))),
        Err(err) => Preview::Blocked(format!("Couldn't run pacman: {err}")),
    }
}

/// Parses `--print-format '%n %v %s'` lines.
pub fn parse_print(stdout: &str) -> Vec<Target> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(Target {
                name: fields.next()?.to_string(),
                version: fields.next()?.to_string(),
                size: fields.next()?.parse().ok()?,
            })
        })
        .collect()
}

/// Keeps pacman's error readable: the first few dependency lines and a count
/// of the rest (removing glibc lists over a thousand).
pub fn blocked_reason(stderr: &str) -> String {
    const SHOWN: usize = 8;
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let mut reason: Vec<String> = lines.iter().take(SHOWN).map(|line| line.to_string()).collect();
    if lines.len() > SHOWN {
        reason.push(format!("…and {} more", lines.len() - SHOWN));
    }
    if reason.is_empty() {
        "pacman refused to remove this.".to_string()
    } else {
        reason.join("\n")
    }
}

/// The command that removes pacman packages (repo or AUR) along with
/// dependencies nothing else needs. pacman still asks `[Y/n]` in the pane.
pub fn pacman_remove(names: &[String]) -> Command {
    Command::new("sudo", ["pacman", "-Rns"].into_iter().map(String::from).chain(names.iter().cloned()))
}

/// The command that uninstalls a Flatpak. System installs need root, and
/// `sudo` keeps that working on setups without a polkit agent.
pub fn flatpak_remove(package: &Package) -> Option<Command> {
    let Source::Flatpak { installation } = &package.source else {
        return None;
    };
    // Custom installations (from /etc/flatpak/installations.d) go by name.
    let target = match installation.as_str() {
        "user" | "system" => format!("--{installation}"),
        other => format!("--installation={other}"),
    };
    let args = ["uninstall", &target, &package.name];
    Some(if installation == "user" {
        Command::new("flatpak", args)
    } else {
        Command::new("sudo", std::iter::once("flatpak").chain(args))
    })
}

/// The command that removes Flatpak runtimes nothing uses anymore, for each
/// installation that has runtimes (`user`, `system`). Flatpak lists what it
/// will remove and asks before doing it. Two installations run back to back
/// in one shell, stopping if the first one fails.
pub fn flatpak_remove_unused(installations: &[&str]) -> Option<Command> {
    let steps: Vec<&str> = ["user", "system"]
        .into_iter()
        .filter(|installation| installations.contains(installation))
        .map(|installation| match installation {
            "user" => "flatpak uninstall --user --unused",
            _ => "sudo flatpak uninstall --system --unused",
        })
        .collect();
    match steps.as_slice() {
        [] => None,
        [only] => {
            let mut words = only.split(' ');
            let program = words.next()?;
            Some(Command::new(program, words))
        }
        _ => Some(Command::new("sh", ["-c".to_string(), steps.join(" && ")])),
    }
}

/// A full system upgrade. pacman packages only ever update this way: `-Sy`
/// plus a single package is a partial upgrade, which Arch doesn't support.
pub fn system_update() -> Command {
    Command::new("sudo", ["pacman", "-Syu"])
}

/// Rebuilds one AUR package. The helper asks for sudo itself when it installs.
pub fn aur_update(helper: AurHelper, name: &str) -> Command {
    Command::new(helper.program(), ["-S", "--aur", name])
}

/// Updates one Flatpak, with `sudo` for system installs like removal does.
pub fn flatpak_update(package: &Package) -> Option<Command> {
    let Source::Flatpak { installation } = &package.source else {
        return None;
    };
    let mut words = flatpak_update_words(installation);
    words.push(package.name.clone());
    Some(command_from(words))
}

/// Installs a repo package. When the system is behind, the install rides
/// along with a full upgrade (`-Syu <pkg>`), never `-Sy <pkg>`, which would be
/// a partial upgrade.
pub fn pacman_install(name: &str, full_upgrade: bool) -> Command {
    let operation = if full_upgrade { "-Syu" } else { "-S" };
    Command::new("sudo", ["pacman", operation, name])
}

/// Builds and installs an AUR package. The helper shows the PKGBUILD and
/// asks for sudo itself.
pub fn aur_install(helper: AurHelper, name: &str) -> Command {
    Command::new(helper.program(), ["-S", "--aur", name])
}

/// Installs a Flatpak from `remote`, with `sudo` for system installs like
/// removal does.
pub fn flatpak_install(remote: &str, id: &str, installation: &str) -> Command {
    let mut words: Vec<String> = Vec::new();
    if installation != "user" {
        words.push("sudo".into());
    }
    words.extend(["flatpak".into(), "install".into(), installation_flag(installation), remote.into(), id.into()]);
    command_from(words)
}

/// Everything at once: a full pacman upgrade, then AUR packages, then each
/// Flatpak installation with updates. Each step only runs if the one before
/// it succeeded.
pub fn update_all(repo: bool, aur: Option<AurHelper>, flatpak_installations: &[&str]) -> Option<Command> {
    let mut steps: Vec<Vec<String>> = Vec::new();
    if repo {
        let update = system_update();
        steps.push(std::iter::once(update.program).chain(update.args).collect());
    }
    if let Some(helper) = aur {
        steps.push(vec![helper.program().into(), "-Sua".into()]);
    }
    steps.extend(flatpak_installations.iter().map(|installation| flatpak_update_words(installation)));
    match steps.len() {
        0 => None,
        1 => steps.pop().map(command_from),
        _ => {
            let script = steps.iter().map(|words| words.join(" ")).collect::<Vec<_>>().join(" && ");
            Some(Command::new("sh", ["-c".to_string(), script]))
        }
    }
}

fn flatpak_update_words(installation: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    if installation != "user" {
        words.push("sudo".into());
    }
    words.extend(["flatpak".into(), "update".into(), installation_flag(installation)]);
    words
}

fn command_from(mut words: Vec<String>) -> Command {
    let program = words.remove(0);
    Command::new(program, words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::category::Category;
    use crate::model::package::Reason;

    #[test]
    fn builds_update_commands() {
        assert_eq!(system_update().display(), "sudo pacman -Syu");
        assert_eq!(aur_update(AurHelper::Yay, "ttt").display(), "yay -S --aur ttt");
        assert!(update_all(false, None, &[]).is_none());
        assert_eq!(update_all(false, Some(AurHelper::Paru), &[]).unwrap().display(), "paru -Sua");
        assert_eq!(
            update_all(true, Some(AurHelper::Paru), &["user", "system"]).unwrap().display(),
            "sh -c sudo pacman -Syu && paru -Sua && flatpak update --user && sudo flatpak update --system"
        );
    }

    #[test]
    fn builds_install_commands() {
        assert_eq!(pacman_install("ripgrep", false).display(), "sudo pacman -S ripgrep");
        assert_eq!(pacman_install("ripgrep", true).display(), "sudo pacman -Syu ripgrep");
        assert_eq!(aur_install(AurHelper::Paru, "code-git").display(), "paru -S --aur code-git");
        assert_eq!(
            flatpak_install("flathub", "md.obsidian.Obsidian", "system").display(),
            "sudo flatpak install --system flathub md.obsidian.Obsidian"
        );
        assert_eq!(
            flatpak_install("dev", "org.example.App", "user").display(),
            "flatpak install --user dev org.example.App"
        );
    }

    #[test]
    fn parses_print_output() {
        let targets = parse_print("firefox 157.0-1.1 313024076\ngcc15-libs 15.3.0-2 909576\n\n");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].name, "firefox");
        assert_eq!(targets[1].size, 909_576);
        assert_eq!(Preview::total_size(&targets), 313_933_652);
    }

    #[test]
    fn trims_long_blocked_reasons() {
        let stderr: String = (0..20).map(|ix| format!(":: removing glibc breaks dependency required by pkg{ix}\n")).collect();
        let reason = blocked_reason(&format!("error: failed to prepare transaction\n{stderr}"));
        assert_eq!(reason.lines().count(), 9);
        assert!(reason.ends_with("…and 13 more"));
    }

    #[test]
    fn builds_remove_commands() {
        let command = pacman_remove(&["firefox".into(), "vlc".into()]);
        assert_eq!(command.display(), "sudo pacman -Rns firefox vlc");

        let mut flatpak = Package {
            name: "org.example.App".into(),
            version: "1".into(),
            description: String::new(),
            source: Source::Flatpak { installation: "user".into() },
            reason: Reason::Explicit,
            size: 0,
            install_date: None,
            url: None,
            licenses: Vec::new(),
            depends: Vec::new(),
            optional_depends: Vec::new(),
            required_by: Vec::new(),
            optional_for: Vec::new(),
            provides: Vec::new(),
            launcher: None,
            category: Category::OtherApps,
            orphan: false,
        };
        assert_eq!(flatpak_remove(&flatpak).unwrap().display(), "flatpak uninstall --user org.example.App");
        flatpak.source = Source::Flatpak { installation: "system".into() };
        assert_eq!(flatpak_remove(&flatpak).unwrap().display(), "sudo flatpak uninstall --system org.example.App");
    }

    #[test]
    fn builds_unused_runtime_commands() {
        assert!(flatpak_remove_unused(&[]).is_none());
        assert_eq!(flatpak_remove_unused(&["user"]).unwrap().display(), "flatpak uninstall --user --unused");
        assert_eq!(
            flatpak_remove_unused(&["system"]).unwrap().display(),
            "sudo flatpak uninstall --system --unused"
        );
        assert_eq!(
            flatpak_remove_unused(&["system", "user"]).unwrap().display(),
            "sh -c flatpak uninstall --user --unused && sudo flatpak uninstall --system --unused"
        );
    }

    /// Runs pacman's real dry run, so it needs an Arch system with pacman.
    #[test]
    fn real_pacman_preview_lists_the_package_itself() {
        if Process::new("pacman").arg("--version").output().is_err() {
            return;
        }
        match preview_pacman_removal(&["pacman".into()]) {
            // Removing pacman is blocked by base on most systems; either way
            // pacman answered and the parse didn't fail.
            Preview::Removes(targets) => assert!(targets.iter().any(|target| target.name == "pacman")),
            Preview::Blocked(reason) => assert!(!reason.is_empty()),
        }
    }
}

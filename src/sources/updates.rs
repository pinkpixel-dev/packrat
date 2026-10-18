//! Checks for pending updates without changing anything. `checkupdates`
//! syncs a throwaway copy of the databases, so the real sync DB stays as it
//! is and nothing here can cause a partial upgrade.

use std::process::{Command, Output};

use crate::model::update::{self, Key, Updates};
use crate::settings::AurHelper;

/// What one check found. Errors are per source, so an offline AUR doesn't
/// hide repo updates.
#[derive(Default)]
pub struct Check {
    pub updates: Updates,
    pub errors: Vec<String>,
}

/// Checks repos, the AUR and Flatpak side by side. Blocking: run it off the
/// UI thread.
pub fn check(helper: Option<AurHelper>) -> Check {
    let (repo, aur, flatpak) = std::thread::scope(|scope| {
        let repo = scope.spawn(repo);
        let aur = scope.spawn(move || helper.map(aur).unwrap_or(Ok(Vec::new())));
        let flatpak = scope.spawn(flatpak);
        (join(repo), join(aur), join(flatpak))
    });

    let mut check = Check::default();
    for result in [repo, aur] {
        match result {
            Ok(found) => {
                for (name, version) in found {
                    check.updates.insert(Key::Pacman(name), version);
                }
            }
            Err(err) => check.errors.push(err),
        }
    }
    let (found, errors) = flatpak;
    for (key, version) in found {
        check.updates.insert(key, version);
    }
    check.errors.extend(errors);
    check
}

fn join<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// `checkupdates` exits 0 with updates, 2 with none, 1 when it failed.
fn repo() -> Result<Vec<(String, String)>, String> {
    let output = run(Command::new("checkupdates").arg("--nocolor"))
        .map_err(|_| "Install pacman-contrib to check for repo updates.".to_string())?;
    match output.status.code() {
        Some(0) => Ok(update::parse_pacman(&String::from_utf8_lossy(&output.stdout))),
        Some(2) => Ok(Vec::new()),
        _ => Err(format!("Couldn't check repo updates: {}", first_line(&output.stderr))),
    }
}

/// `-Qua` exits 1 when nothing needs updating, so only an `error:` line
/// counts as failing.
fn aur(helper: AurHelper) -> Result<Vec<(String, String)>, String> {
    let program = helper.program();
    let output = run(Command::new(program).arg("-Qua"))
        .map_err(|err| format!("Couldn't run {program}: {err}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && stderr.contains("error") {
        return Err(format!("Couldn't check AUR updates: {}", first_line(&output.stderr)));
    }
    Ok(update::parse_pacman(&String::from_utf8_lossy(&output.stdout)))
}

/// One remote at a time: a single broken remote makes the combined listing
/// fail outright.
fn flatpak() -> (Vec<(Key, String)>, Vec<String>) {
    let Ok(output) = run(Command::new("flatpak").args(["remotes", "--columns=name,options"])) else {
        return (Vec::new(), Vec::new());
    };
    let remotes = update::parse_remotes(&String::from_utf8_lossy(&output.stdout));

    let mut found = Vec::new();
    let mut errors = Vec::new();
    for remote in remotes {
        let output = run(Command::new("flatpak")
            .args(["remote-ls", &installation_flag(&remote.installation), "--updates"])
            .args(["--columns=application,version,branch", &remote.name]));
        match output {
            Ok(output) if output.status.success() => {
                found.extend(update::parse_flatpak(&String::from_utf8_lossy(&output.stdout), &remote.installation));
            }
            Ok(output) if remote.enumerable => errors.push(format!(
                "Couldn't check Flatpak remote {}: {}",
                remote.name,
                first_line(&output.stderr)
            )),
            _ => {}
        }
    }
    (found, errors)
}

/// `--user`, `--system`, or `--installation=NAME` for custom installations.
pub fn installation_flag(installation: &str) -> String {
    match installation {
        "user" | "system" => format!("--{installation}"),
        other => format!("--installation={other}"),
    }
}

fn run(command: &mut Command) -> std::io::Result<Output> {
    command.env("LC_ALL", "C").output()
}

fn first_line(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("no details");
    line.trim_start_matches("error: ").trim_start_matches("==> ERROR: ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_useful_part_of_errors() {
        assert_eq!(first_line(b"\nerror: failed to sync\nmore"), "failed to sync");
        assert_eq!(first_line(b"==> ERROR: Cannot fetch updates"), "Cannot fetch updates");
        assert_eq!(first_line(b""), "no details");
        assert_eq!(installation_flag("extra"), "--installation=extra");
    }
}

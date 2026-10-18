//! Searches for packages to install. Nothing here changes the system:
//! `pacman -Ss` reads the sync databases already on disk, the AUR goes
//! through its official RPC API, and `flatpak search` reads each remote's
//! cached appstream data.

use std::process::{Command, Output};

use crate::model::candidate::{self, Candidate};
use crate::model::update;

const AUR_RPC: &str = "https://aur.archlinux.org/rpc/v5/search/";
/// The AUR refuses searches shorter than this.
pub const MIN_QUERY: usize = 2;

/// What one search found, best matches first. Errors are per source, so an
/// offline AUR doesn't hide repo results.
#[derive(Default)]
pub struct Found {
    pub candidates: Vec<Candidate>,
    pub errors: Vec<String>,
}

/// Searches the repos, the AUR and Flatpak side by side. Blocking: run it
/// off the UI thread.
pub fn search(query: &str) -> Found {
    let query = query.trim();
    if query.chars().count() < MIN_QUERY {
        return Found::default();
    }
    let results = std::thread::scope(|scope| {
        let repo = scope.spawn(|| repo(query));
        let aur = scope.spawn(|| aur(query));
        let flatpak = scope.spawn(|| flatpak(query));
        [join(repo), join(aur), join(flatpak)]
    });

    let mut found = Found::default();
    for result in results {
        match result {
            Ok(candidates) => found.candidates.extend(candidates),
            Err(err) => found.errors.push(err),
        }
    }
    candidate::sort(&mut found.candidates, query);
    found
}

fn join<T>(handle: std::thread::ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// pacman reads each word as a regex and wants all of them to match. Words
/// are escaped so `c++` searches for `c++`. Exit 1 means nothing matched.
fn repo(query: &str) -> Result<Vec<Candidate>, String> {
    let output = run(Command::new("pacman").arg("-Ss").args(query.split_whitespace().map(escape_regex)))
        .map_err(|err| format!("Couldn't run pacman: {err}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && !stderr.trim().is_empty() {
        return Err(format!("Couldn't search the repos: {}", stderr.lines().next().unwrap_or_default()));
    }
    Ok(candidate::parse_pacman(&String::from_utf8_lossy(&output.stdout)))
}

/// The AUR RPC through curl, which pacman itself depends on.
fn aur(query: &str) -> Result<Vec<Candidate>, String> {
    let url = format!("{AUR_RPC}{}?by=name-desc", percent_encode(query));
    let output = run(Command::new("curl")
        .args(["--silent", "--show-error", "--max-time", "15"])
        .args(["--user-agent", concat!("packrat/", env!("CARGO_PKG_VERSION"))])
        .arg(url))
    .map_err(|err| format!("Couldn't run curl: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.trim().trim_start_matches("curl: ").split(") ").last().unwrap_or_default().to_string();
        return Err(format!("Couldn't reach the AUR: {reason}"));
    }
    candidate::parse_aur(&String::from_utf8_lossy(&output.stdout)).map_err(|err| format!("AUR: {err}"))
}

/// Every configured remote at once. Without Flatpak there's simply nothing
/// to show.
fn flatpak(query: &str) -> Result<Vec<Candidate>, String> {
    let Ok(remotes) = run(Command::new("flatpak").args(["remotes", "--columns=name,options"])) else {
        return Ok(Vec::new());
    };
    let remotes = update::parse_remotes(&String::from_utf8_lossy(&remotes.stdout));
    let output = run(Command::new("flatpak")
        .args(["search", "--columns=application,name,description,version,branch,remotes"])
        .arg(query))
    .map_err(|err| format!("Couldn't run flatpak: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Couldn't search Flatpak: {}", stderr.lines().next().unwrap_or_default()));
    }
    Ok(candidate::parse_flatpak(&String::from_utf8_lossy(&output.stdout), &remotes))
}

fn run(command: &mut Command) -> std::io::Result<Output> {
    command.env("LC_ALL", "C").output()
}

/// Escapes the characters POSIX extended regexes treat as special.
fn escape_regex(word: &str) -> String {
    let mut escaped = String::with_capacity(word.len());
    for char in word.chars() {
        if r"\.^$|?*+()[]{}".contains(char) {
            escaped.push('\\');
        }
        escaped.push(char);
    }
    escaped
}

/// Percent-encodes everything but unreserved URL characters.
fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_regex_and_urls() {
        assert_eq!(escape_regex("c++"), r"c\+\+");
        assert_eq!(escape_regex("lib.so"), r"lib\.so");
        assert_eq!(percent_encode("visual studio code"), "visual%20studio%20code");
        assert_eq!(percent_encode("c++/é"), "c%2B%2B%2F%C3%A9");
    }

    #[test]
    fn short_queries_find_nothing() {
        assert!(search(" a ").candidates.is_empty());
    }

    /// Runs pacman's real search, so it needs an Arch system with pacman.
    #[test]
    fn real_repo_search_finds_pacman() {
        if Command::new("pacman").arg("--version").output().is_err() {
            return;
        }
        let found = repo("pacman").unwrap();
        assert!(found.iter().any(|candidate| candidate.name == "pacman"));
        assert!(repo("c++").is_ok());
    }
}

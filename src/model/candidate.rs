//! Packages you don't have yet, found by searching the repos, the AUR and
//! Flatpak remotes, and parsing what each search prints.

use serde_json::Value;

use super::package::Source;
use super::update::Remote;

/// Extra facts the AUR RPC returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AurInfo {
    pub votes: u64,
    pub popularity: f64,
    /// `None` for orphaned packages.
    pub maintainer: Option<String>,
    pub out_of_date: bool,
    pub url: Option<String>,
}

/// Something a search found that you could install.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The package name, or the app ID for a Flatpak.
    pub name: String,
    /// Flatpak's display name, like "Obsidian" for `md.obsidian.Obsidian`.
    pub title: Option<String>,
    pub version: String,
    pub description: String,
    /// Where it comes from. For a Flatpak, the installation it would go to.
    pub source: Source,
    /// The Flatpak remote to install from.
    pub remote: Option<String>,
    pub aur: Option<AurInfo>,
}

impl Candidate {
    pub fn display_name(&self) -> &str {
        self.title.as_deref().filter(|title| !title.is_empty()).unwrap_or(&self.name)
    }
}

/// Parses `pacman -Ss`: a `repo/name version [installed]` line, then the
/// description indented below it. A package in several repos is listed once,
/// from the first one, which is the one `pacman -S` would pick.
pub fn parse_pacman(stdout: &str) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = Vec::new();
    // Whether the description lines belong to a repeat that was skipped.
    let mut skipping = false;
    for line in stdout.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(last) = found.last_mut().filter(|last| !skipping && last.description.is_empty()) {
                last.description = line.trim().to_string();
            }
            continue;
        }
        let mut fields = line.split_whitespace();
        let (Some(id), Some(version)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Some((repo, name)) = id.split_once('/') else {
            continue;
        };
        skipping = found.iter().any(|candidate| candidate.name == name);
        if skipping {
            continue;
        }
        found.push(Candidate {
            name: name.to_string(),
            title: None,
            version: version.to_string(),
            description: String::new(),
            source: Source::Repo(repo.to_string()),
            remote: None,
            aur: None,
        });
    }
    found
}

/// Parses an AUR RPC v5 search response. RPC errors (like "Too many package
/// results.") come back as `Err`.
pub fn parse_aur(json: &str) -> Result<Vec<Candidate>, String> {
    let value: Value = serde_json::from_str(json).map_err(|_| "the AUR sent something unreadable".to_string())?;
    if value.get("type").and_then(Value::as_str) == Some("error") {
        let error = value.get("error").and_then(Value::as_str).unwrap_or("unknown error");
        return Err(error.trim_end_matches('.').to_string());
    }
    let text = |result: &Value, key: &str| result.get(key).and_then(Value::as_str).map(str::to_string);
    let results = value.get("results").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(results
        .iter()
        .filter_map(|result| {
            Some(Candidate {
                name: text(result, "Name")?,
                title: None,
                version: text(result, "Version").unwrap_or_default(),
                description: text(result, "Description").unwrap_or_default(),
                source: Source::Aur,
                remote: None,
                aur: Some(AurInfo {
                    votes: result.get("NumVotes").and_then(Value::as_u64).unwrap_or(0),
                    popularity: result.get("Popularity").and_then(Value::as_f64).unwrap_or(0.),
                    maintainer: text(result, "Maintainer"),
                    out_of_date: result.get("OutOfDate").is_some_and(|date| !date.is_null()),
                    url: text(result, "URL"),
                }),
            })
        })
        .collect())
}

/// Parses `flatpak search --columns=application,name,description,version,branch,remotes`.
/// Each result installs system-wide when its remote is set up there,
/// otherwise wherever the remote is.
pub fn parse_flatpak(stdout: &str, remotes: &[Remote]) -> Vec<Candidate> {
    let mut found: Vec<Candidate> = Vec::new();
    for line in stdout.lines() {
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [id, title, description, version, branch, remote_list, ..] = fields.as_slice() else {
            continue;
        };
        if id.is_empty() || found.iter().any(|candidate| candidate.name == *id) {
            continue;
        }
        let Some((remote, installation)) = remote_list.split(',').map(str::trim).find_map(|remote| {
            install_target(remote, remotes).map(|installation| (remote.to_string(), installation))
        }) else {
            continue;
        };
        found.push(Candidate {
            name: id.to_string(),
            title: Some(title.to_string()).filter(|title| !title.is_empty()),
            version: if version.is_empty() { branch } else { version }.to_string(),
            description: description.to_string(),
            source: Source::Flatpak { installation },
            remote: Some(remote),
            aur: None,
        });
    }
    found
}

/// The installation a Flatpak from `remote` goes to: `system` when the
/// remote is configured there, otherwise the first installation that has it.
fn install_target(remote: &str, remotes: &[Remote]) -> Option<String> {
    let mut matching = remotes.iter().filter(|candidate| candidate.name == remote);
    let first = matching.clone().next()?;
    Some(if matching.any(|candidate| candidate.installation == "system") {
        "system".to_string()
    } else {
        first.installation.clone()
    })
}

/// How well a candidate matches a search: exact names first, then name
/// prefixes, then names containing it, then descriptions that mention every
/// word. Anything else the tool matched on its own terms comes last.
pub fn rank(candidate: &Candidate, query: &str) -> u8 {
    let query = query.trim().to_lowercase();
    let name = candidate.name.to_lowercase();
    let title = candidate.display_name().to_lowercase();
    let description = candidate.description.to_lowercase();
    if name == query || title == query {
        0
    } else if name.starts_with(&query) || title.starts_with(&query) {
        1
    } else if name.contains(&query) || title.contains(&query) {
        2
    } else if query
        .split_whitespace()
        .all(|word| name.contains(word) || title.contains(word) || description.contains(word))
    {
        3
    } else {
        4
    }
}

/// Puts the best matches first. Within a rank, repo packages come before the
/// AUR and Flatpak, and AUR packages with more votes come first.
pub fn sort(candidates: &mut [Candidate], query: &str) {
    candidates.sort_by_cached_key(|candidate| {
        let source = match candidate.source {
            Source::Repo(_) => 0,
            Source::Flatpak { .. } => 1,
            Source::Aur => 2,
        };
        let votes = candidate.aur.as_ref().map_or(0, |aur| aur.votes);
        (rank(candidate, query), source, std::cmp::Reverse(votes), candidate.display_name().to_lowercase())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pacman_search_once_per_name() {
        let found = parse_pacman(
            "cachyos-v3/ripgrep 15.2.0-1 [installed]\n    A search tool\n\
             extra/ripgrep 15.2.0-1 [installed]\n    A search tool\n\
             extra/ripgrep-all 0.10.10-3 (group)\n    rga: ripgrep, but also PDFs\n",
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "ripgrep");
        assert_eq!(found[0].source, Source::Repo("cachyos-v3".into()));
        assert_eq!(found[0].description, "A search tool");
        assert_eq!(found[1].name, "ripgrep-all");
        assert_eq!(found[1].description, "rga: ripgrep, but also PDFs");
    }

    #[test]
    fn parses_aur_results_and_errors() {
        let found = parse_aur(
            r#"{"resultcount":1,"type":"search","version":5,"results":[{"Name":"code-git","Version":"1.1-1",
            "Description":"OSS build","NumVotes":16,"Popularity":0.5,"Maintainer":null,"OutOfDate":1700000000,
            "URL":"https://github.com/microsoft/vscode"}]}"#,
        )
        .unwrap();
        assert_eq!(found[0].name, "code-git");
        let aur = found[0].aur.as_ref().unwrap();
        assert_eq!(aur.votes, 16);
        assert!(aur.out_of_date);
        assert_eq!(aur.maintainer, None);

        let error = parse_aur(r#"{"error":"Query arg too small.","resultcount":0,"results":[],"type":"error","version":5}"#);
        assert_eq!(error, Err("Query arg too small".to_string()));
    }

    #[test]
    fn parses_flatpak_search_and_prefers_system() {
        let remotes = [
            Remote { name: "flathub".into(), installation: "user".into(), enumerable: true },
            Remote { name: "flathub".into(), installation: "system".into(), enumerable: true },
            Remote { name: "dev".into(), installation: "user".into(), enumerable: false },
        ];
        let found = parse_flatpak(
            "md.obsidian.Obsidian\tObsidian\tMarkdown-based knowledge base\t1.14.4\tstable\tflathub\n\
             org.example.Dev\tDev\tLocal build\t\tmaster\tdev\n\
             org.example.Gone\tGone\tRemote removed\t1\tstable\tgone\n\
             No matches found\n",
            &remotes,
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].display_name(), "Obsidian");
        assert_eq!(found[0].source, Source::Flatpak { installation: "system".into() });
        assert_eq!(found[0].remote.as_deref(), Some("flathub"));
        assert_eq!(found[1].version, "master");
        assert_eq!(found[1].source, Source::Flatpak { installation: "user".into() });
    }

    #[test]
    fn ranks_exact_names_before_themes() {
        let remotes = [Remote { name: "flathub".into(), installation: "system".into(), enumerable: true }];
        let mut found = parse_flatpak(
            "org.gtk.Gtk3theme.Obsidian-2\tObsidian-2 Gtk Theme\tA theme\t2.25\t3.22\tflathub\n\
             md.obsidian.Obsidian\tObsidian\tMarkdown-based knowledge base\t1.14.4\tstable\tflathub\n",
            &remotes,
        );
        found.extend(parse_pacman("extra/notes 1-1\n    Plain text notes, like obsidian\n"));
        sort(&mut found, "Obsidian");
        let names: Vec<&str> = found.iter().map(|candidate| candidate.name.as_str()).collect();
        assert_eq!(names, ["md.obsidian.Obsidian", "org.gtk.Gtk3theme.Obsidian-2", "notes"]);
        assert_eq!(rank(&found[2], "obsidian"), 3);
    }
}

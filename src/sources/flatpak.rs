//! Installed Flatpak apps and runtimes, from the `flatpak` CLI.

use std::path::PathBuf;
use std::process::Command;

use crate::model::category::{Category, app_category};
use crate::model::launcher;
use crate::model::package::{Package, Reason, Source};

const COLUMNS: &str = "--columns=application,name,version,branch,origin,installation,size,description";

/// Every installed Flatpak. Empty when Flatpak isn't installed.
pub fn load() -> Vec<Package> {
    let mut packages = list("--app", false);
    packages.extend(list("--runtime", true));
    packages
}

fn list(kind: &str, runtime: bool) -> Vec<Package> {
    let Ok(output) = Command::new("flatpak").args(["list", kind, COLUMNS]).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| parse_line(line, runtime))
        .collect()
}

pub fn parse_line(line: &str, runtime: bool) -> Option<Package> {
    let fields: Vec<&str> = line.split('\t').collect();
    let field = |ix: usize| fields.get(ix).map(|value| value.trim()).unwrap_or_default().to_string();
    let id = field(0);
    if id.is_empty() {
        return None;
    }
    let installation = field(5);
    let version = match (field(2), field(3)) {
        (version, _) if !version.is_empty() => version,
        (_, branch) => branch,
    };

    let launcher = if runtime { None } else { exported_launcher(&id, &installation) };
    let category = match (&launcher, runtime) {
        (_, true) => Category::Runtimes,
        (Some(launcher), _) => app_category(&launcher.categories),
        (None, _) => Category::OtherApps,
    };
    let mut launcher = launcher;
    // Flatpak already gives a friendly name; keep it when the launcher has none.
    if let Some(launcher) = &mut launcher
        && launcher.name.is_empty()
    {
        launcher.name = field(1);
    }

    Some(Package {
        description: Some(field(7)).filter(|desc| !desc.is_empty()).unwrap_or_else(|| field(1)),
        version,
        source: Source::Flatpak { installation: installation.clone() },
        // Apps are things you chose; runtimes come along with them.
        reason: if runtime { Reason::Dependency } else { Reason::Explicit },
        size: parse_size(&field(6)),
        install_date: None,
        url: None,
        licenses: Vec::new(),
        depends: Vec::new(),
        optional_depends: Vec::new(),
        required_by: Vec::new(),
        optional_for: Vec::new(),
        provides: Vec::new(),
        launcher,
        category,
        orphan: false,
        name: id,
    })
}

/// The launcher Flatpak exports for an app, in the system or user installation.
fn exported_launcher(id: &str, installation: &str) -> Option<crate::model::package::Launcher> {
    let base = if installation == "user" {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share/flatpak"))?
    } else {
        PathBuf::from("/var/lib/flatpak")
    };
    launcher::read(&base.join("exports/share/applications").join(format!("{id}.desktop")))
}

/// Parses Flatpak's human sizes (`287.8 MB`, `3.5 GB`, `512 bytes`). Flatpak
/// uses powers of 1000.
pub fn parse_size(text: &str) -> u64 {
    let text = text.replace('\u{a0}', " ");
    let mut parts = text.split_whitespace();
    let Some(value) = parts.next().and_then(|value| value.parse::<f64>().ok()) else {
        return 0;
    };
    let scale = match parts.next().unwrap_or("").to_ascii_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.,
    };
    (value * scale) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_list_lines() {
        let app = parse_line(
            "chat.revolt.RevoltDesktop\tRevolt\t1.0.8\tstable\tflathub\tsystem\t287.8 MB\tOpen source messaging",
            false,
        )
        .unwrap();
        assert_eq!(app.name, "chat.revolt.RevoltDesktop");
        assert_eq!(app.version, "1.0.8");
        assert_eq!(app.size, 287_800_000);
        assert_eq!(app.description, "Open source messaging");
        assert_eq!(app.source, Source::Flatpak { installation: "system".into() });

        let runtime = parse_line("org.freedesktop.Platform\tFreedesktop Platform\t\t25.08\tflathub\tuser\t659.3 MB", true).unwrap();
        assert_eq!(runtime.version, "25.08");
        assert_eq!(runtime.category, Category::Runtimes);
        assert_eq!(runtime.reason, Reason::Dependency);
    }

    #[test]
    fn parses_sizes() {
        assert_eq!(parse_size("3.5\u{a0}GB"), 3_500_000_000);
        assert_eq!(parse_size("512 bytes"), 512);
        assert_eq!(parse_size(""), 0);
    }
}

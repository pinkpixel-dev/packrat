use super::category::Category;

/// Where a package came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A pacman repo, named (`core`, `extra`, `cachyos-extra-v3`, ...).
    Repo(String),
    /// Installed by pacman but not in any configured repo: the AUR, or a
    /// package built locally.
    Aur,
    /// A Flatpak, with its installation (`system` or `user`).
    Flatpak { installation: String },
}

/// The source filter in the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    Repo,
    Aur,
    Flatpak,
}

impl Source {
    pub fn kind(&self) -> SourceKind {
        match self {
            Source::Repo(_) => SourceKind::Repo,
            Source::Aur => SourceKind::Aur,
            Source::Flatpak { .. } => SourceKind::Flatpak,
        }
    }

    /// Short text for the list badge.
    pub fn badge(&self) -> &str {
        match self {
            Source::Repo(repo) => repo,
            Source::Aur => "AUR",
            Source::Flatpak { .. } => "Flatpak",
        }
    }
}

/// Why a pacman package is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// You asked for it.
    Explicit,
    /// Pulled in by something else.
    Dependency,
}

/// What a `.desktop` launcher says about a package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launcher {
    /// The name shown in app menus, like "Firefox" for `firefox`.
    pub name: String,
    /// The freedesktop `Categories=` list.
    pub categories: Vec<String>,
}

/// Everything installed, from any source, in one shape.
#[derive(Clone, Debug)]
pub struct Package {
    /// The package name, or the app ID for a Flatpak.
    pub name: String,
    pub version: String,
    pub description: String,
    pub source: Source,
    pub reason: Reason,
    /// Bytes on disk.
    pub size: u64,
    /// Unix seconds, when known.
    pub install_date: Option<i64>,
    pub url: Option<String>,
    pub licenses: Vec<String>,
    /// Dependencies as pacman lists them, constraints included (`glibc>=2.38`).
    pub depends: Vec<String>,
    /// `name: why` lines, as pacman prints them.
    pub optional_depends: Vec<String>,
    /// Everything that needs this package, filled in after loading.
    pub required_by: Vec<String>,
    /// Everything that lists this package as an optional dependency.
    pub optional_for: Vec<String>,
    /// Names this package also answers to, with versions (`libssl.so=3-64`).
    pub provides: Vec<String>,
    pub launcher: Option<Launcher>,
    pub category: Category,
    /// Installed as a dependency and nothing needs it anymore, even
    /// optionally. The same set as `pacman -Qdt`.
    pub orphan: bool,
}

impl Package {
    /// The friendliest name to show: the launcher name when it says more
    /// than the package name does.
    pub fn display_name(&self) -> &str {
        match &self.launcher {
            Some(launcher) if !launcher.name.is_empty() => &launcher.name,
            _ => &self.name,
        }
    }
}

/// Removes a version constraint: `glibc>=2.38` becomes `glibc`.
pub fn strip_constraint(dep: &str) -> &str {
    dep.split(['<', '>', '=', ':']).next().unwrap_or(dep).trim()
}

/// Human file size: `312.4 MB`.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A Unix timestamp as a calendar date, in UTC: `2026-10-07`.
pub fn format_date(secs: i64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_version_constraints() {
        assert_eq!(strip_constraint("glibc>=2.38"), "glibc");
        assert_eq!(strip_constraint("python=3.13"), "python");
        assert_eq!(strip_constraint("sh"), "sh");
        assert_eq!(strip_constraint("libfoo.so=1-64"), "libfoo.so");
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format_date(0), "1970-01-01");
        assert_eq!(format_date(1_791_123_135), "2026-10-04");
        assert_eq!(format_date(951_782_400), "2000-02-29");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_500), "1.5 KB");
        assert_eq!(format_size(313_024_076), "313.0 MB");
    }
}

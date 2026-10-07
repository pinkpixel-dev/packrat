use std::collections::HashMap;

use super::package::{Package, Reason, SourceKind, strip_constraint};

/// Everything installed, with dependency links filled in.
pub struct Inventory {
    pub packages: Vec<Package>,
}

impl Inventory {
    pub fn new(mut packages: Vec<Package>) -> Self {
        link_dependencies(&mut packages);
        packages.sort_by_cached_key(|package| package.name.to_lowercase());
        Self { packages }
    }
}

/// Fills in `required_by`, `optional_for` and `orphan` for pacman packages.
/// A dependency can be met by a package's name or anything it `provides`.
fn link_dependencies(packages: &mut [Package]) {
    // Provided name -> (package index, provided version, if any).
    let mut providers: HashMap<&str, Vec<(usize, Option<&str>)>> = HashMap::new();
    for (ix, package) in packages.iter().enumerate() {
        if package.source.kind() == SourceKind::Flatpak {
            continue;
        }
        providers.entry(&package.name).or_default().push((ix, None));
        for provided in &package.provides {
            let (name, version) = match provided.split_once('=') {
                Some((name, version)) => (name, Some(version)),
                None => (provided.as_str(), None),
            };
            providers.entry(name).or_default().push((ix, version));
        }
    }

    let mut required_by: Vec<Vec<String>> = vec![Vec::new(); packages.len()];
    let mut optional_for: Vec<Vec<String>> = vec![Vec::new(); packages.len()];
    for package in packages.iter() {
        for dep in &package.depends {
            for ix in satisfiers(&providers, dep) {
                required_by[ix].push(package.name.clone());
            }
        }
        for line in &package.optional_depends {
            let dep = line.split_once(": ").map_or(line.as_str(), |(dep, _)| dep);
            for ix in satisfiers(&providers, dep) {
                optional_for[ix].push(package.name.clone());
            }
        }
    }

    for (ix, package) in packages.iter_mut().enumerate() {
        let mut required = std::mem::take(&mut required_by[ix]);
        required.sort();
        required.dedup();
        required.retain(|name| *name != package.name);
        let mut optional = std::mem::take(&mut optional_for[ix]);
        optional.sort();
        optional.dedup();

        package.orphan = package.source.kind() != SourceKind::Flatpak
            && package.reason == Reason::Dependency
            && required.is_empty()
            && optional.is_empty();
        package.required_by = required;
        package.optional_for = optional;
    }
}

/// Packages that satisfy a dependency string like `libssl.so=3-64`.
///
/// An exact `=` constraint has to match a provided version exactly, which is
/// what keeps `openssl-1.1` (`libssl.so=1.1-64`) from looking needed by
/// everything that links the current `libssl.so=3-64`. Ranges (`>=`, `<`) only
/// check the name: installed packages already met them when pacman resolved them.
fn satisfiers(providers: &HashMap<&str, Vec<(usize, Option<&str>)>>, dep: &str) -> Vec<usize> {
    let name = strip_constraint(dep);
    let exact = match dep.find(['<', '>', '=']) {
        Some(at) if dep[at..].starts_with('=') => Some(&dep[at + 1..]),
        _ => None,
    };
    providers
        .get(name)
        .into_iter()
        .flatten()
        .filter(|(_, provided)| match (exact, provided) {
            (Some(wanted), Some(provided)) => wanted == *provided,
            _ => true,
        })
        .map(|(ix, _)| *ix)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::category::Category;
    use crate::model::package::Source;

    fn package(name: &str, reason: Reason, depends: &[&str], provides: &[&str]) -> Package {
        Package {
            name: name.into(),
            version: "1".into(),
            description: String::new(),
            source: Source::Repo("extra".into()),
            reason,
            size: 0,
            install_date: None,
            url: None,
            licenses: Vec::new(),
            depends: depends.iter().map(|dep| dep.to_string()).collect(),
            optional_depends: Vec::new(),
            required_by: Vec::new(),
            optional_for: Vec::new(),
            provides: provides.iter().map(|dep| dep.to_string()).collect(),
            launcher: None,
            category: Category::Other,
            orphan: false,
        }
    }

    #[test]
    fn links_through_provides_and_finds_orphans() {
        let mut app = package("app", Reason::Explicit, &["libfoo", "sh"], &[]);
        app.optional_depends = vec!["helper: extra feature".into()];
        let inventory = Inventory::new(vec![
            app,
            package("libfoo", Reason::Dependency, &[], &[]),
            package("bash", Reason::Dependency, &[], &["sh"]),
            package("helper", Reason::Dependency, &[], &[]),
            package("leftover", Reason::Dependency, &[], &[]),
        ]);
        let get = |name: &str| inventory.packages.iter().find(|package| package.name == name).unwrap();

        assert_eq!(get("libfoo").required_by, ["app"]);
        assert_eq!(get("bash").required_by, ["app"]);
        assert_eq!(get("helper").optional_for, ["app"]);
        assert!(!get("helper").orphan);
        assert!(get("leftover").orphan);
        assert!(!get("app").orphan);
    }

    #[test]
    fn exact_soname_versions_must_match() {
        let app = package("app", Reason::Explicit, &["libssl.so=3-64"], &[]);
        let inventory = Inventory::new(vec![
            app,
            package("openssl", Reason::Dependency, &[], &["libssl.so=3-64"]),
            package("openssl-1.1", Reason::Dependency, &[], &["libssl.so=1.1-64"]),
        ]);
        let get = |name: &str| inventory.packages.iter().find(|package| package.name == name).unwrap();
        assert_eq!(get("openssl").required_by, ["app"]);
        assert!(get("openssl-1.1").orphan);
    }
}

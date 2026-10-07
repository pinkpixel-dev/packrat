use super::category::Category;
use super::package::{Package, Reason, SourceKind};

/// The sidebar selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    All,
    Apps,
    Explicit,
    Orphans,
    Category(Category),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Name,
    /// Biggest first.
    Size,
    /// Newest first.
    Installed,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Sort::Name => "Name",
            Sort::Size => "Size",
            Sort::Installed => "Recently Installed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Filter {
    pub view: View,
    pub source: Option<SourceKind>,
    pub query: String,
    pub sort: Sort,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            view: View::Apps,
            source: None,
            query: String::new(),
            sort: Sort::Name,
        }
    }
}

impl Filter {
    /// Whether a package shows up for this view, ignoring source and search.
    pub fn in_view(view: View, package: &Package) -> bool {
        match view {
            View::All => true,
            View::Apps => package.category.is_app(),
            View::Explicit => package.reason == Reason::Explicit,
            View::Orphans => package.orphan,
            View::Category(category) => package.category == category,
        }
    }

    /// Indices into `packages` that match, in display order.
    pub fn apply(&self, packages: &[Package]) -> Vec<usize> {
        let query = self.query.trim().to_lowercase();
        let mut matches: Vec<usize> = packages
            .iter()
            .enumerate()
            .filter(|(_, package)| Self::in_view(self.view, package))
            .filter(|(_, package)| self.source.is_none_or(|kind| package.source.kind() == kind))
            .filter(|(_, package)| query.is_empty() || search_rank(package, &query).is_some())
            .map(|(ix, _)| ix)
            .collect();

        matches.sort_by(|&a, &b| {
            let (a, b) = (&packages[a], &packages[b]);
            let rank = if query.is_empty() {
                std::cmp::Ordering::Equal
            } else {
                search_rank(a, &query).cmp(&search_rank(b, &query))
            };
            rank.then_with(|| match self.sort {
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Size => b.size.cmp(&a.size),
                Sort::Installed => b.install_date.cmp(&a.install_date),
            })
            .then_with(|| a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()))
        });
        matches
    }
}

/// How well a package matches a lowercase query: lower is better, `None` is no
/// match. Exact names beat name prefixes, which beat any other hit.
fn search_rank(package: &Package, query: &str) -> Option<u8> {
    let name = package.name.to_lowercase();
    let display = package.display_name().to_lowercase();
    if name == query || display == query {
        Some(0)
    } else if name.starts_with(query) || display.starts_with(query) {
        Some(1)
    } else if name.contains(query) || display.contains(query) {
        Some(2)
    } else if package.description.to_lowercase().contains(query) {
        Some(3)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::package::Source;

    fn package(name: &str, description: &str, size: u64) -> Package {
        Package {
            name: name.into(),
            version: "1".into(),
            description: description.into(),
            source: Source::Repo("extra".into()),
            reason: Reason::Explicit,
            size,
            install_date: None,
            url: None,
            licenses: Vec::new(),
            depends: Vec::new(),
            optional_depends: Vec::new(),
            required_by: Vec::new(),
            optional_for: Vec::new(),
            provides: Vec::new(),
            launcher: None,
            category: Category::CommandLine,
            orphan: false,
        }
    }

    #[test]
    fn search_ranks_names_before_descriptions() {
        let packages = vec![
            package("fd", "find alternative using git rules", 1),
            package("git-lfs", "", 1),
            package("git", "", 1),
            package("tig", "text interface for git", 1),
        ];
        let filter = Filter {
            view: View::All,
            query: "git".into(),
            ..Filter::default()
        };
        let names: Vec<&str> = filter.apply(&packages).iter().map(|&ix| packages[ix].name.as_str()).collect();
        assert_eq!(names, ["git", "git-lfs", "fd", "tig"]);
    }

    #[test]
    fn filters_by_view_and_sorts_by_size() {
        let mut orphan = package("leftover", "", 5);
        orphan.orphan = true;
        let packages = vec![package("small", "", 1), package("big", "", 100), orphan];
        let filter = Filter {
            view: View::All,
            sort: Sort::Size,
            ..Filter::default()
        };
        let names: Vec<&str> = filter.apply(&packages).iter().map(|&ix| packages[ix].name.as_str()).collect();
        assert_eq!(names, ["big", "leftover", "small"]);

        let orphans = Filter {
            view: View::Orphans,
            ..Filter::default()
        };
        assert_eq!(orphans.apply(&packages), [2]);
    }
}

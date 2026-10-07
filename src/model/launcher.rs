use std::path::Path;

use super::package::Launcher;

/// Reads a `.desktop` file and returns its launcher, or `None` when it isn't
/// a visible application (missing, `NoDisplay`, `Hidden`, or not `Type=Application`).
pub fn read(path: &Path) -> Option<Launcher> {
    parse(&std::fs::read_to_string(path).ok()?)
}

pub fn parse(contents: &str) -> Option<Launcher> {
    let mut in_entry = false;
    let mut name = None;
    let mut categories = Vec::new();
    let mut is_app = false;

    for line in contents.lines().map(str::trim) {
        if line.starts_with('[') {
            // Only the main group counts; actions come after it.
            if in_entry {
                break;
            }
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_string()),
            "Categories" => {
                categories = value
                    .split(';')
                    .map(str::trim)
                    .filter(|category| !category.is_empty())
                    .map(String::from)
                    .collect()
            }
            "Type" => is_app = value.trim() == "Application",
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }

    is_app.then(|| Launcher {
        name: name.unwrap_or_default(),
        categories,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_main_entry() {
        let launcher = parse(
            "[Desktop Entry]\nType=Application\nName=Firefox\nName[de]=Feuerfuchs\nCategories=Network;WebBrowser;\n\n\
             [Desktop Action new-window]\nName=New Window\n",
        )
        .unwrap();
        assert_eq!(launcher.name, "Firefox");
        assert_eq!(launcher.categories, ["Network", "WebBrowser"]);
    }

    #[test]
    fn skips_hidden_and_non_apps() {
        assert!(parse("[Desktop Entry]\nType=Application\nName=X\nNoDisplay=true\n").is_none());
        assert!(parse("[Desktop Entry]\nType=Link\nName=X\n").is_none());
        assert!(parse("Name=X\n").is_none());
    }
}

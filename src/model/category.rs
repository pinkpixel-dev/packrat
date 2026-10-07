use gpui_kit::assets::IconName;

/// What a package is, for browsing. Arch doesn't tag packages this way, so
/// [`classify`] works it out from the launcher, the name and the file list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    // Apps, from the launcher's freedesktop categories.
    Development,
    Graphics,
    Internet,
    Multimedia,
    Office,
    Games,
    Education,
    Science,
    System,
    Utilities,
    Settings,
    OtherApps,
    // Everything without a launcher.
    CommandLine,
    Services,
    Libraries,
    Python,
    LanguageModules,
    Fonts,
    Themes,
    KernelDrivers,
    Documentation,
    Runtimes,
    Other,
}

impl Category {
    /// Sidebar order.
    pub const ALL: [Category; 23] = [
        Category::Development,
        Category::Graphics,
        Category::Internet,
        Category::Multimedia,
        Category::Office,
        Category::Games,
        Category::Education,
        Category::Science,
        Category::System,
        Category::Utilities,
        Category::Settings,
        Category::OtherApps,
        Category::CommandLine,
        Category::Services,
        Category::Libraries,
        Category::Python,
        Category::LanguageModules,
        Category::Fonts,
        Category::Themes,
        Category::KernelDrivers,
        Category::Documentation,
        Category::Runtimes,
        Category::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Development => "Development",
            Category::Graphics => "Graphics",
            Category::Internet => "Internet",
            Category::Multimedia => "Multimedia",
            Category::Office => "Office",
            Category::Games => "Games",
            Category::Education => "Education",
            Category::Science => "Science",
            Category::System => "System",
            Category::Utilities => "Utilities",
            Category::Settings => "Settings",
            Category::OtherApps => "Other Apps",
            Category::CommandLine => "Command Line",
            Category::Services => "Services",
            Category::Libraries => "Libraries",
            Category::Python => "Python",
            Category::LanguageModules => "Language Modules",
            Category::Fonts => "Fonts",
            Category::Themes => "Themes & Icons",
            Category::KernelDrivers => "Kernel & Drivers",
            Category::Documentation => "Documentation",
            Category::Runtimes => "Flatpak Runtimes",
            Category::Other => "Config & Data",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Category::Development => IconName::Code,
            Category::Graphics => IconName::Image,
            Category::Internet => IconName::Globe,
            Category::Multimedia => IconName::Clapperboard,
            Category::Office => IconName::FileText,
            Category::Games => IconName::Gamepad2,
            Category::Education => IconName::GraduationCap,
            Category::Science => IconName::FlaskConical,
            Category::System => IconName::MonitorCog,
            Category::Utilities => IconName::Wrench,
            Category::Settings => IconName::SlidersHorizontal,
            Category::OtherApps => IconName::AppWindow,
            Category::CommandLine => IconName::Terminal,
            Category::Services => IconName::Server,
            Category::Libraries => IconName::Blocks,
            Category::Python => IconName::Braces,
            Category::LanguageModules => IconName::Braces,
            Category::Fonts => IconName::Type,
            Category::Themes => IconName::Paintbrush,
            Category::KernelDrivers => IconName::Cpu,
            Category::Documentation => IconName::BookOpen,
            Category::Runtimes => IconName::Boxes,
            Category::Other => IconName::Shapes,
        }
    }

    /// Whether this is one of the launcher-based app categories.
    pub fn is_app(self) -> bool {
        matches!(
            self,
            Category::Development
                | Category::Graphics
                | Category::Internet
                | Category::Multimedia
                | Category::Office
                | Category::Games
                | Category::Education
                | Category::Science
                | Category::System
                | Category::Utilities
                | Category::Settings
                | Category::OtherApps
        )
    }
}

/// Maps a launcher's `Categories=` list to an app category. The first main
/// category in the list wins, since apps list their primary one first.
pub fn app_category(categories: &[String]) -> Category {
    categories
        .iter()
        .find_map(|category| match category.as_str() {
            "Development" => Some(Category::Development),
            "Graphics" => Some(Category::Graphics),
            "Network" => Some(Category::Internet),
            "AudioVideo" | "Audio" | "Video" => Some(Category::Multimedia),
            "Office" => Some(Category::Office),
            "Game" => Some(Category::Games),
            "Education" => Some(Category::Education),
            "Science" => Some(Category::Science),
            "System" => Some(Category::System),
            "Utility" => Some(Category::Utilities),
            "Settings" => Some(Category::Settings),
            _ => None,
        })
        .unwrap_or(Category::OtherApps)
}

/// Prefixes for packages of language libraries, apart from Python.
const LANGUAGE_PREFIXES: &[&str] = &[
    "perl-", "ruby-", "haskell-", "lua-", "lua51-", "lua52-", "lua53-", "lua54-", "nodejs-", "r-", "ocaml-",
    "php-", "java-", "dotnet-",
];

/// Driver packages that don't ship kernel modules themselves.
const DRIVER_PREFIXES: &[&str] = &["nvidia", "lib32-nvidia", "mesa", "lib32-mesa", "vulkan-", "lib32-vulkan-", "xf86-video-", "xf86-input-"];

/// Sorts a pacman package without a launcher, from its name and the paths it
/// installs (as listed in the local database, without a leading slash).
pub fn classify(name: &str, files: &[String], launcher_categories: Option<&[String]>) -> Category {
    if let Some(categories) = launcher_categories {
        return app_category(categories);
    }

    let has = |prefix: &str| files.iter().any(|file| file.starts_with(prefix) && file.len() > prefix.len());

    if has("usr/lib/modules/")
        || has("usr/lib/firmware/")
        || name.ends_with("-dkms")
        || name.ends_with("-firmware")
        || DRIVER_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
    {
        return Category::KernelDrivers;
    }
    if name.starts_with("ttf-")
        || name.starts_with("otf-")
        || name.ends_with("-fonts")
        || name.contains("-fonts-")
        || name.starts_with("noto-fonts")
        || mostly_under(files, "usr/share/fonts/")
    {
        return Category::Fonts;
    }
    if name.ends_with("-docs") || name.ends_with("-doc") {
        return Category::Documentation;
    }
    if name.starts_with("python-") || name.starts_with("python2-") {
        return Category::Python;
    }
    if LANGUAGE_PREFIXES.iter().any(|prefix| name.starts_with(prefix)) {
        return Category::LanguageModules;
    }

    let has_bins = has("usr/bin/");
    if !has_bins
        && (has("usr/share/icons/") || has("usr/share/themes/") || name.ends_with("-theme") || name.contains("-icon-theme"))
    {
        return Category::Themes;
    }
    if name.starts_with("lib") {
        return Category::Libraries;
    }
    if files
        .iter()
        .any(|file| file.starts_with("usr/lib/systemd/system/") && file.ends_with(".service"))
    {
        return Category::Services;
    }
    if has_bins {
        return Category::CommandLine;
    }
    if has("usr/include/") || files.iter().any(|file| file.contains(".so")) {
        return Category::Libraries;
    }
    Category::Other
}

/// True when most installed files (not directories) sit under `prefix`.
fn mostly_under(files: &[String], prefix: &str) -> bool {
    let regular: Vec<&String> = files.iter().filter(|file| !file.ends_with('/')).collect();
    if regular.is_empty() {
        return false;
    }
    let under = regular.iter().filter(|file| file.starts_with(prefix)).count();
    under * 2 > regular.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| path.to_string()).collect()
    }

    #[test]
    fn launcher_categories_pick_the_first_main_one() {
        let categories = files(&["Network", "WebBrowser"]);
        assert_eq!(classify("firefox", &[], Some(&categories)), Category::Internet);
        let categories = files(&["Qt", "KDE", "Utility"]);
        assert_eq!(classify("kate", &[], Some(&categories)), Category::Utilities);
        let categories = files(&["X-Custom"]);
        assert_eq!(classify("thing", &[], Some(&categories)), Category::OtherApps);
    }

    #[test]
    fn sorts_packages_without_launchers() {
        assert_eq!(classify("linux-cachyos", &files(&["usr/lib/modules/6.1/vmlinuz"]), None), Category::KernelDrivers);
        assert_eq!(classify("nvidia-utils", &files(&["usr/bin/nvidia-smi"]), None), Category::KernelDrivers);
        assert_eq!(classify("ttf-jetbrains-mono", &[], None), Category::Fonts);
        assert_eq!(classify("python-requests", &[], None), Category::Python);
        assert_eq!(classify("perl-error", &[], None), Category::LanguageModules);
        assert_eq!(classify("papirus-icon-theme", &files(&["usr/share/icons/Papirus/"]), None), Category::Themes);
        assert_eq!(classify("libnotify", &files(&["usr/bin/notify-send"]), None), Category::Libraries);
        assert_eq!(
            classify("openssh", &files(&["usr/bin/ssh", "usr/lib/systemd/system/sshd.service"]), None),
            Category::Services
        );
        assert_eq!(classify("ripgrep", &files(&["usr/bin/rg"]), None), Category::CommandLine);
        assert_eq!(classify("glslang", &files(&["usr/include/glslang/", "usr/lib/libglslang.so"]), None), Category::Libraries);
        assert_eq!(classify("ca-certificates", &files(&["etc/ssl/certs/"]), None), Category::Other);
    }

    #[test]
    fn mostly_fonts_counts_files_not_directories() {
        let paths = files(&["usr/", "usr/share/", "usr/share/fonts/", "usr/share/fonts/a.ttf", "usr/share/fonts/b.ttf", "usr/share/licenses/x/LICENSE"]);
        assert!(mostly_under(&paths, "usr/share/fonts/"));
    }
}

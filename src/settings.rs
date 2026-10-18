//! Preferences that survive a restart, saved as JSON in
//! `$XDG_CONFIG_HOME/packrat/settings.json`.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The program that checks and builds AUR packages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AurHelper {
    Paru,
    Yay,
}

impl AurHelper {
    pub const ALL: [AurHelper; 2] = [AurHelper::Paru, AurHelper::Yay];

    pub fn program(self) -> &'static str {
        match self {
            AurHelper::Paru => "paru",
            AurHelper::Yay => "yay",
        }
    }

    fn from_program(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|helper| helper.program() == name)
    }

    /// The helpers found on `PATH`, in preference order.
    pub fn installed() -> Vec<AurHelper> {
        Self::ALL.into_iter().filter(|helper| on_path(helper.program())).collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// `None` until you pick one: paru, or yay when paru isn't installed.
    pub aur_helper: Option<AurHelper>,
}

impl Settings {
    /// Reads the settings file. A missing or broken file means defaults.
    pub fn load() -> Self {
        path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let value: Value = serde_json::from_str(text).unwrap_or_default();
        Self {
            aur_helper: value.get("aur_helper").and_then(Value::as_str).and_then(AurHelper::from_program),
        }
    }

    pub fn to_json(&self) -> String {
        let mut map = Map::new();
        if let Some(helper) = self.aur_helper {
            map.insert("aur_helper".into(), helper.program().into());
        }
        serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path().ok_or_else(|| std::io::Error::other("no config directory (HOME isn't set)"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, self.to_json())
    }

    /// The helper to actually use: your pick when it's installed, otherwise
    /// whichever one is.
    pub fn aur_helper(&self, installed: &[AurHelper]) -> Option<AurHelper> {
        self.aur_helper
            .filter(|helper| installed.contains(helper))
            .or_else(|| installed.first().copied())
    }
}

fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("packrat/settings.json"))
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| is_executable(&dir.join(program))))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_helper() {
        let settings = Settings { aur_helper: Some(AurHelper::Yay) };
        assert_eq!(Settings::parse(&settings.to_json()), settings);
        assert_eq!(Settings::parse("not json"), Settings::default());
        assert_eq!(Settings::parse(r#"{"aur_helper": "pikaur"}"#), Settings::default());
    }

    #[test]
    fn falls_back_to_an_installed_helper() {
        let yay = Settings { aur_helper: Some(AurHelper::Yay) };
        assert_eq!(yay.aur_helper(&[AurHelper::Paru, AurHelper::Yay]), Some(AurHelper::Yay));
        assert_eq!(yay.aur_helper(&[AurHelper::Paru]), Some(AurHelper::Paru));
        assert_eq!(Settings::default().aur_helper(&[AurHelper::Paru, AurHelper::Yay]), Some(AurHelper::Paru));
        assert_eq!(Settings::default().aur_helper(&[]), None);
    }
}

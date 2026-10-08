use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub interval_secs: u64,
    pub always_on_top: bool,
    pub start_at_login: bool,
    pub x: i32,
    pub y: i32,
    pub codex_path: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            interval_secs: 60,
            always_on_top: true,
            start_at_login: false,
            x: 160,
            y: 160,
            codex_path: None,
        }
    }
}
impl Config {
    pub fn path() -> Result<PathBuf> {
        Ok(
            PathBuf::from(std::env::var_os("LOCALAPPDATA").context("缺少 LOCALAPPDATA")?)
                .join("NanfengCodexQuota/settings.toml"),
        )
    }
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (30..=600).contains(&self.interval_secs),
            "刷新间隔须在 30–600 秒之间"
        );
        anyhow::ensure!(self.version == 1, "设置来自较新版本，当前版本不能覆盖");
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let config: Self = toml::from_str(&fs::read_to_string(path).context("读取设置失败")?)
            .context("设置文件损坏，已保留原文件")?;
        config.validate()?;
        Ok(config)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path.parent().context("设置目录无效")?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("toml.tmp");
        use std::io::Write;
        let mut file = fs::File::create(&temporary)?;
        file.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        file.sync_all()?;
        drop(file);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            // MoveFileExW atomically replaces the destination, preserving the old file on failure.
            unsafe extern "system" {
                fn MoveFileExW(a: *const u16, b: *const u16, flags: u32) -> i32;
            }
            let a: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 1 | 8) } == 0 {
                return Err(std::io::Error::last_os_error()).context("保存设置失败，原设置未覆盖");
            }
        }
        #[cfg(not(windows))]
        fs::rename(temporary, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_config_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "broken [").unwrap();
        assert!(Config::load(&path).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "broken [");
    }
    #[test]
    fn persists_settings_and_position_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let config = Config {
            x: -800,
            y: 220,
            interval_secs: 120,
            ..Default::default()
        };
        config.save(&path).unwrap();
        let new = Config {
            interval_secs: 30,
            ..config
        };
        new.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), new);
    }
}

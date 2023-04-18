use std::path::PathBuf;
use cfg_if::cfg_if;
use serde::{Deserialize, Serialize};
use color_eyre::Result;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::env::var;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ICalConfig {
    pub url: String,
    pub name: String,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Config {
    pub ical: Vec<ICalConfig>,
}

impl Config {
    cfg_if! {
        if #[cfg(target_os = "linux")] {
            async fn get_path() -> Result<PathBuf> {
                let home = PathBuf::from(var("HOME")?);
                let dest_dir = home
                    .join(".local")
                    .join("hour-calc");

                if !dest_dir.exists() {
                    fs::create_dir_all(&dest_dir).await?;
                }

                Ok(dest_dir.join("config.json"))
            }
        } else if #[cfg(windows)] {
            async fn get_path() -> Result<PathBuf> {
                let home = PathBuf::from(var("APPDATA")?);
                let dest_dir = home
                    .join("hour-calc");

                if !dest_dir.exists() {
                    fs::create_dir_all(&dest_dir).await?;
                }

                Ok(dest_dir.join("config.json"))
            }
        } else {
            compiler_error!("Unsupported platform");
        }
    }

    pub async fn clear() -> Result<()> {
        let path = Self::get_path().await?;
        fs::remove_file(&path).await?;
        Ok(())
    }

    pub async fn open() -> Result<Option<Self>> {
        let path = Self::get_path().await?;
        if !path.exists() {
            return Ok(None);
        }

        let mut f = fs::File::open(&path).await?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).await?;

        let this: Self = serde_json::from_slice(&buf)?;
        Ok(Some(this))
    }

    pub async fn store(&self) -> Result<()> {
        let path = Self::get_path().await?;
        let mut f = fs::File::create(&path).await?;

        let buf = serde_json::to_vec_pretty(self)?;
        f.write(&buf).await?;
        Ok(())
    }
}
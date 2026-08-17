use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub project: ProjectConfig,
    #[serde(default)]
    pub surface: Vec<SurfaceConfig>,
    pub github: Option<GitHubConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version_source: PathBuf,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub install: Option<String>,
    pub release_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SurfaceConfig {
    pub name: String,
    pub kind: SurfaceKind,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    #[serde(default)]
    pub require: Vec<String>,
    pub canonical: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Markdown,
    Url,
    Rss,
    Sitemap,
    Llms,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GitHubConfig {
    /// `owner/repository`, not a URL.
    pub repository: String,
    pub homepage: Option<String>,
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub check_latest_release: bool,
}

const fn default_true() -> bool {
    true
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("cannot read configuration {}", path.display()))?;
        let config: Self = toml::from_str(&raw)
            .with_context(|| format!("invalid configuration {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.project.name.trim().is_empty() {
            bail!("project.name cannot be empty");
        }
        if self.surface.is_empty() && self.github.is_none() {
            bail!("configure at least one [[surface]] or [github] check");
        }
        for surface in &self.surface {
            match (&surface.path, &surface.url) {
                (Some(_), Some(_)) => {
                    bail!("surface '{}' must set path or url, not both", surface.name)
                }
                (None, None) => bail!("surface '{}' must set path or url", surface.name),
                _ => {}
            }
            if surface.kind == SurfaceKind::Markdown && surface.path.is_none() {
                bail!("markdown surface '{}' must use path", surface.name);
            }
            if surface.kind == SurfaceKind::Llms && surface.url.is_none() {
                bail!("llms surface '{}' must use url", surface.name);
            }
        }
        Ok(())
    }
}

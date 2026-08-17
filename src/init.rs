use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde_json::Value as JsonValue;

pub const DEFAULT_CONFIG: &str = "surfacecheck.toml";

pub fn create(root: &Path, output: &Path, force: bool) -> Result<PathBuf> {
    let destination = root.join(output);
    if destination.exists() && !force {
        bail!(
            "{} already exists; use --force to replace it",
            destination.display()
        );
    }

    let manifest = detect_manifest(root)?;
    let facts = manifest_facts(&manifest)?;
    let git_remote = git_remote(root);
    let repository = facts.repository.or(git_remote);
    let github_slug = repository.as_deref().and_then(github_slug);

    let mut document = String::new();
    document.push_str("# Public release facts that must stay consistent.\n\n");
    document.push_str("[project]\n");
    document.push_str(&format!("name = {}\n", toml_string(&facts.name)));
    document.push_str(&format!(
        "version_source = {}\n",
        toml_string(
            manifest
                .strip_prefix(root)
                .unwrap_or(&manifest)
                .to_string_lossy()
                .as_ref()
        )
    ));
    if let Some(repository) = &repository {
        document.push_str(&format!("repository = {}\n", toml_string(repository)));
    }
    if let Some(install) = &facts.install {
        document.push_str(&format!("install = {}\n", toml_string(install)));
    }

    if root.join("README.md").exists() {
        document.push_str("\n[[surface]]\n");
        document.push_str("name = \"README\"\nkind = \"markdown\"\npath = \"README.md\"\n");
        let mut required = vec!["{version}"];
        if facts.install.is_some() {
            required.push("{install}");
        }
        document.push_str(&format!("require = {:?}\n", required));
    }

    if let Some(slug) = github_slug {
        document.push_str("\n[github]\n");
        document.push_str(&format!("repository = {}\n", toml_string(&slug)));
        document.push_str("check_latest_release = true\n");
    }

    fs::write(&destination, document)
        .with_context(|| format!("cannot write {}", destination.display()))?;
    Ok(destination)
}

struct ManifestFacts {
    name: String,
    repository: Option<String>,
    install: Option<String>,
}

fn detect_manifest(root: &Path) -> Result<PathBuf> {
    ["Cargo.toml", "package.json", "pyproject.toml"]
        .iter()
        .map(|name| root.join(name))
        .find(|path| path.exists())
        .context("no Cargo.toml, package.json, or pyproject.toml found")
}

fn manifest_facts(path: &Path) -> Result<ManifestFacts> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("cannot read manifest {}", path.display()))?;
    match path.file_name().and_then(|name| name.to_str()) {
        Some("Cargo.toml") => {
            let manifest: toml::Value = toml::from_str(&raw).context("invalid Cargo.toml")?;
            let package = manifest
                .get("package")
                .context("Cargo.toml has no [package]")?;
            let name = package
                .get("name")
                .and_then(toml::Value::as_str)
                .context("Cargo.toml has no package.name")?
                .to_owned();
            let repository = package
                .get("repository")
                .and_then(toml::Value::as_str)
                .map(str::to_owned);
            Ok(ManifestFacts {
                install: Some(format!("cargo install {name}")),
                name,
                repository,
            })
        }
        Some("package.json") => {
            let manifest: JsonValue = serde_json::from_str(&raw).context("invalid package.json")?;
            let name = manifest
                .get("name")
                .and_then(JsonValue::as_str)
                .context("package.json has no name")?
                .to_owned();
            let repository = manifest.get("repository").and_then(|repository| {
                repository
                    .as_str()
                    .or_else(|| repository.get("url").and_then(JsonValue::as_str))
            });
            Ok(ManifestFacts {
                install: Some(format!("npm install {name}")),
                name,
                repository: repository.map(str::to_owned),
            })
        }
        Some("pyproject.toml") => {
            let manifest: toml::Value = toml::from_str(&raw).context("invalid pyproject.toml")?;
            let project = manifest
                .get("project")
                .or_else(|| manifest.get("tool").and_then(|tool| tool.get("poetry")))
                .context("pyproject.toml has no [project] or [tool.poetry]")?;
            let name = project
                .get("name")
                .and_then(toml::Value::as_str)
                .context("pyproject.toml has no project name")?
                .to_owned();
            Ok(ManifestFacts {
                install: Some(format!("pip install {name}")),
                name,
                repository: None,
            })
        }
        _ => unreachable!("manifest detection controls file names"),
    }
}

fn git_remote(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn github_slug(remote: &str) -> Option<String> {
    let trimmed = remote.trim_end_matches('/').trim_end_matches(".git");
    if let Some(slug) = trimmed.strip_prefix("git@github.com:") {
        return valid_slug(slug);
    }
    if let Some(slug) = trimmed.strip_prefix("https://github.com/") {
        return valid_slug(slug);
    }
    None
}

fn valid_slug(slug: &str) -> Option<String> {
    (slug.split('/').count() == 2).then(|| slug.to_owned())
}

fn toml_string(value: &str) -> String {
    format!("{:?}", value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_github_slug() {
        assert_eq!(
            github_slug("git@github.com:tcballard/project.git").as_deref(),
            Some("tcballard/project")
        );
        assert_eq!(
            github_slug("https://github.com/tcballard/project").as_deref(),
            Some("tcballard/project")
        );
    }
}

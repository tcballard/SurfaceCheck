use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use regex::Regex;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use serde::Deserialize;

use crate::config::{Config, GitHubConfig, ProjectConfig, SurfaceConfig, SurfaceKind};
use crate::model::{Finding, Report};

const USER_AGENT_VALUE: &str = "surfacecheck/0.1 (+https://github.com/tcballard/surfacecheck)";

pub fn run(config: &Config, root: &Path, offline: bool) -> Result<Report> {
    let version = read_version(&root.join(&config.project.version_source))?;
    let variables = variables(&config.project, &version);
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("cannot build HTTP client")?;
    let mut findings = Vec::new();

    for surface in &config.surface {
        check_surface(surface, root, &client, &variables, offline, &mut findings);
    }

    if let Some(github) = &config.github {
        if offline {
            findings.push(Finding::pass(
                "github.skipped",
                "GitHub",
                "skipped in offline mode",
            ));
        } else {
            check_github(github, &client, &version, &mut findings);
        }
    }

    Ok(Report::new(config.project.name.clone(), version, findings))
}

fn check_surface(
    surface: &SurfaceConfig,
    root: &Path,
    client: &Client,
    variables: &BTreeMap<&str, String>,
    offline: bool,
    findings: &mut Vec<Finding>,
) {
    if offline && surface.url.is_some() {
        findings.push(Finding::pass(
            "surface.skipped",
            &surface.name,
            "skipped in offline mode",
        ));
        return;
    }

    let loaded = if let Some(path) = &surface.path {
        let full_path = root.join(path);
        fs::read_to_string(&full_path)
            .map(|body| LoadedSurface {
                body,
                content_type: None,
                final_url: None,
            })
            .with_context(|| format!("cannot read {}", full_path.display()))
    } else if let Some(url) = &surface.url {
        load_url(client, url)
    } else {
        unreachable!("configuration validation requires a source")
    };

    let loaded = match loaded {
        Ok(loaded) => {
            findings.push(Finding::pass(
                "surface.reachable",
                &surface.name,
                "surface is readable",
            ));
            loaded
        }
        Err(error) => {
            findings.push(Finding::fail(
                "surface.unreachable",
                &surface.name,
                "surface could not be read",
                Some(format!("{error:#}")),
            ));
            return;
        }
    };

    for required in &surface.require {
        let expanded = expand(required, variables);
        if loaded.body.contains(&expanded) {
            findings.push(Finding::pass(
                "content.required",
                &surface.name,
                format!("contains required fact: {expanded}"),
            ));
        } else {
            findings.push(Finding::fail(
                "content.missing",
                &surface.name,
                format!("missing required fact: {expanded}"),
                Some(surface_location(surface)),
            ));
        }
    }

    if let Some(expected) = &surface.canonical {
        let expected = expand(expected, variables);
        match canonical_url(&loaded.body) {
            Some(actual) if urls_equal(&actual, &expected) => findings.push(Finding::pass(
                "html.canonical",
                &surface.name,
                format!("canonical URL is {actual}"),
            )),
            Some(actual) => findings.push(Finding::fail(
                "html.canonical_mismatch",
                &surface.name,
                format!("canonical URL does not match {expected}"),
                Some(format!("found {actual}")),
            )),
            None => findings.push(Finding::fail(
                "html.canonical_missing",
                &surface.name,
                "canonical URL is missing",
                Some(surface_location(surface)),
            )),
        }
    }

    if let (Some(requested), Some(final_url)) = (&surface.url, &loaded.final_url) {
        if !urls_equal(requested, final_url) {
            findings.push(Finding::fail(
                "http.redirect",
                &surface.name,
                "URL redirected to a different public location",
                Some(format!("{requested} -> {final_url}")),
            ));
        } else {
            findings.push(Finding::pass(
                "http.redirect",
                &surface.name,
                "URL did not redirect",
            ));
        }
    }

    if surface.kind == SurfaceKind::Llms {
        check_llms_headers(surface, loaded.content_type.as_deref(), findings);
    }
}

struct LoadedSurface {
    body: String,
    content_type: Option<String>,
    final_url: Option<String>,
}

fn load_url(client: &Client, url: &str) -> Result<LoadedSurface> {
    let response = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .with_context(|| format!("request failed for {url}"))?;
    response.error_for_status_ref().with_context(|| {
        format!(
            "{} returned HTTP {}",
            response.url(),
            response.status().as_u16()
        )
    })?;
    response_to_loaded(response)
}

fn response_to_loaded(response: Response) -> Result<LoadedSurface> {
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let final_url = response.url().as_str().to_owned();
    let body = response.text().context("cannot read HTTP response body")?;
    Ok(LoadedSurface {
        body,
        content_type,
        final_url: Some(final_url),
    })
}

fn check_llms_headers(
    surface: &SurfaceConfig,
    content_type: Option<&str>,
    findings: &mut Vec<Finding>,
) {
    let Some(content_type) = content_type else {
        findings.push(Finding::fail(
            "llms.content_type",
            &surface.name,
            "llms.txt has no Content-Type header",
            None,
        ));
        return;
    };
    let normalized = content_type.to_ascii_lowercase();
    if normalized.contains("text/plain") && normalized.contains("charset=utf-8") {
        findings.push(Finding::pass(
            "llms.content_type",
            &surface.name,
            "Content-Type is text/plain with UTF-8 charset",
        ));
    } else {
        findings.push(Finding::fail(
            "llms.content_type",
            &surface.name,
            "llms.txt must use text/plain with UTF-8 charset",
            Some(content_type.to_owned()),
        ));
    }
}

#[derive(Deserialize)]
struct GitHubRepository {
    description: Option<String>,
    homepage: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

fn check_github(
    config: &GitHubConfig,
    client: &Client,
    version: &str,
    findings: &mut Vec<Finding>,
) {
    let base = format!("https://api.github.com/repos/{}", config.repository);
    match github_get(client, &base).and_then(|response| {
        response
            .json::<GitHubRepository>()
            .context("invalid GitHub repository response")
    }) {
        Ok(repository) => {
            findings.push(Finding::pass(
                "github.reachable",
                "GitHub",
                format!("repository {} is readable", config.repository),
            ));
            check_github_metadata(config, &repository, findings);
        }
        Err(error) => {
            findings.push(Finding::fail(
                "github.unreachable",
                "GitHub",
                format!("repository {} could not be read", config.repository),
                Some(format!("{error:#}")),
            ));
            return;
        }
    }

    if config.check_latest_release {
        let release_url = format!("{base}/releases/latest");
        match github_get(client, &release_url).and_then(|response| {
            response
                .json::<GitHubRelease>()
                .context("invalid GitHub release response")
        }) {
            Ok(release) if versions_equal(&release.tag_name, version) => {
                findings.push(Finding::pass(
                    "github.release",
                    "GitHub",
                    format!("latest release is {}", release.tag_name),
                ));
            }
            Ok(release) => findings.push(Finding::fail(
                "github.release_mismatch",
                "GitHub",
                format!(
                    "latest release {} does not match manifest version {version}",
                    release.tag_name
                ),
                Some(release.html_url),
            )),
            Err(error) => findings.push(Finding::fail(
                "github.release_missing",
                "GitHub",
                "latest GitHub release could not be read",
                Some(format!("{error:#}")),
            )),
        }
    }
}

fn github_get(client: &Client, url: &str) -> Result<Response> {
    let mut request = client
        .get(url)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = request
        .send()
        .with_context(|| format!("GitHub request failed for {url}"))?;
    response
        .error_for_status()
        .with_context(|| format!("GitHub request returned an error for {url}"))
}

fn check_github_metadata(
    expected: &GitHubConfig,
    actual: &GitHubRepository,
    findings: &mut Vec<Finding>,
) {
    if let Some(description) = &expected.description {
        if actual.description.as_deref() == Some(description.as_str()) {
            findings.push(Finding::pass(
                "github.description",
                "GitHub",
                "repository description matches",
            ));
        } else {
            findings.push(Finding::fail(
                "github.description_mismatch",
                "GitHub",
                "repository description does not match",
                Some(format!(
                    "expected {description:?}, found {:?}",
                    actual.description
                )),
            ));
        }
    }
    if let Some(homepage) = &expected.homepage {
        if actual
            .homepage
            .as_deref()
            .is_some_and(|actual| urls_equal(actual, homepage))
        {
            findings.push(Finding::pass(
                "github.homepage",
                "GitHub",
                "repository homepage matches",
            ));
        } else {
            findings.push(Finding::fail(
                "github.homepage_mismatch",
                "GitHub",
                "repository homepage does not match",
                Some(format!(
                    "expected {homepage:?}, found {:?}",
                    actual.homepage
                )),
            ));
        }
    }
}

fn variables(project: &ProjectConfig, version: &str) -> BTreeMap<&'static str, String> {
    let mut values = BTreeMap::new();
    values.insert("version", version.to_owned());
    values.insert("name", project.name.clone());
    if let Some(value) = &project.repository {
        values.insert("repository", value.clone());
    }
    if let Some(value) = &project.homepage {
        values.insert("homepage", value.clone());
    }
    if let Some(value) = &project.install {
        values.insert("install", value.clone());
    }
    if let Some(value) = &project.release_url {
        values.insert("release_url", value.clone());
    }
    values
}

pub fn expand(template: &str, values: &BTreeMap<&str, String>) -> String {
    values
        .iter()
        .fold(template.to_owned(), |expanded, (key, value)| {
            expanded.replace(&format!("{{{key}}}"), value)
        })
}

fn read_version(path: &Path) -> Result<String> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("cannot read version source {}", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    match file_name {
        "Cargo.toml" => {
            let value: toml::Value = toml::from_str(&raw).context("invalid Cargo.toml")?;
            value
                .get("package")
                .and_then(|package| package.get("version"))
                .or_else(|| {
                    value
                        .get("workspace")
                        .and_then(|workspace| workspace.get("package"))
                        .and_then(|package| package.get("version"))
                })
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .context("Cargo.toml has no package.version or workspace.package.version")
        }
        "package.json" => {
            let value: serde_json::Value =
                serde_json::from_str(&raw).context("invalid package.json")?;
            value
                .get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .context("package.json has no version")
        }
        "pyproject.toml" => {
            let value: toml::Value = toml::from_str(&raw).context("invalid pyproject.toml")?;
            value
                .get("project")
                .and_then(|project| project.get("version"))
                .or_else(|| {
                    value
                        .get("tool")
                        .and_then(|tool| tool.get("poetry"))
                        .and_then(|poetry| poetry.get("version"))
                })
                .and_then(toml::Value::as_str)
                .map(str::to_owned)
                .context("pyproject.toml has no project.version or tool.poetry.version")
        }
        _ => bail!(
            "unsupported version source {}; use Cargo.toml, package.json, or pyproject.toml",
            path.display()
        ),
    }
}

fn canonical_url(html: &str) -> Option<String> {
    let tag_pattern = Regex::new(r#"(?is)<link\b[^>]*>"#).expect("valid link regex");
    let rel_pattern =
        Regex::new(r#"(?i)\brel\s*=\s*[\"']canonical[\"']"#).expect("valid rel regex");
    let href_pattern =
        Regex::new(r#"(?i)\bhref\s*=\s*[\"']([^\"']+)[\"']"#).expect("valid href regex");
    let canonical = tag_pattern.find_iter(html).find_map(|tag| {
        let tag = tag.as_str();
        if rel_pattern.is_match(tag) {
            href_pattern
                .captures(tag)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().to_owned())
        } else {
            None
        }
    });
    canonical
}

fn surface_location(surface: &SurfaceConfig) -> String {
    surface
        .path
        .as_ref()
        .map(|path| path.display().to_string())
        .or_else(|| surface.url.clone())
        .unwrap_or_else(|| surface.name.clone())
}

fn urls_equal(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}

fn versions_equal(left: &str, right: &str) -> bool {
    left.trim().trim_start_matches('v') == right.trim().trim_start_matches('v')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_canonical_with_any_attribute_order() {
        let html = r#"<html><head><link href="https://example.com/a" data-x="1" rel="canonical"></head></html>"#;
        assert_eq!(
            canonical_url(html).as_deref(),
            Some("https://example.com/a")
        );
    }

    #[test]
    fn expands_known_variables() {
        let values = BTreeMap::from([("version", "1.2.3".to_owned())]);
        assert_eq!(expand("release v{version}", &values), "release v1.2.3");
    }

    #[test]
    fn version_comparison_allows_v_prefix() {
        assert!(versions_equal("v1.2.3", "1.2.3"));
        assert!(!versions_equal("v1.2.2", "1.2.3"));
    }
}

# Surfacecheck

**CI for the facts users, search engines, and agents read.**

Surfacecheck is a deterministic Rust CLI that checks whether a software
project's public surfaces agree with the release that actually shipped. It
compares the version in the project manifest with claims in repository files,
live websites, release feeds, sitemaps, `llms.txt`, and GitHub metadata.

It does not use an LLM. Every failure includes the fact that was expected and
the file or URL that contradicted it.

## The failure it catches

```text
$ surfacecheck check
demo 2.0.0

PASS  README                       surface is readable
FAIL  README                       missing required fact: 2.0.0
      evidence: README.md
PASS  Website                      canonical URL is https://example.com
FAIL  Release feed                 missing required fact: 2.0.0
      evidence: https://example.com/rss.xml

2 passed, 2 failed
```

Exit code `0` means all configured facts agree. Exit code `1` means one or more
checks failed. Exit code `2` means the configuration or command is invalid.

## Install

```bash
cargo install surfacecheck
```

Surfacecheck builds with the current stable Rust toolchain.

## Quick start

Run this in a Rust, Node.js, or Python project:

```bash
surfacecheck init
surfacecheck check --offline
```

`init` detects `Cargo.toml`, `package.json`, or `pyproject.toml`. It creates a
small `surfacecheck.toml` from the manifest, README, installation command, Git
remote, and GitHub repository.

Then add the live surfaces that matter:

```toml
[project]
name = "demo"
version_source = "Cargo.toml"
homepage = "https://demo.example"
install = "cargo install demo"
release_url = "https://github.com/example/demo/releases/tag/v{version}"

[[surface]]
name = "README"
kind = "markdown"
path = "README.md"
require = ["{version}", "{install}"]

[[surface]]
name = "Website"
kind = "url"
url = "https://demo.example"
require = ["{version}"]
canonical = "{homepage}"

[[surface]]
name = "Release feed"
kind = "rss"
url = "https://demo.example/rss.xml"
require = ["{version}", "{release_url}"]

[[surface]]
name = "Sitemap"
kind = "sitemap"
url = "https://demo.example/sitemap.xml"
require = ["{homepage}"]

[[surface]]
name = "llms.txt"
kind = "llms"
url = "https://demo.example/llms.txt"
require = ["{name}", "{version}"]

[github]
repository = "example/demo"
homepage = "https://demo.example"
description = "What the GitHub About panel must say"
check_latest_release = true
```

The supported variables are `{name}`, `{version}`, `{repository}`, `{homepage}`,
`{install}`, and `{release_url}`. Surfacecheck expands them in `require` and
`canonical` values.

## Checks in 0.1

- Required release facts in Markdown and public text
- HTTP availability and unexpected redirects
- Exact HTML canonical URLs
- Release presence in RSS or Atom feeds
- Required public URLs in sitemaps
- `llms.txt` content and `text/plain; charset=utf-8` delivery
- GitHub description and homepage consistency
- Latest GitHub release against the manifest version
- Human and JSON reports
- Offline mode for repository-only CI

`GITHUB_TOKEN` is optional for public repositories and recommended in CI to
avoid anonymous API rate limits.

## GitHub Actions

The repository includes a composite action. After Surfacecheck is published,
a consuming repository can use:

```yaml
- uses: tcballard/surfacecheck@v0.1.0
  with:
    config: surfacecheck.toml
```

For development, run the same gates as CI:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Deliberate limits

Surfacecheck is not a general SEO crawler or release publisher. Version 0.1
does not mutate websites, update GitHub settings, infer claims with an LLM, or
follow an entire site. Maintainers declare the facts and surfaces that define a
complete release; Surfacecheck checks them exactly.

## License

MIT

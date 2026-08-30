use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

use tempfile::TempDir;

fn binary() -> Command {
    Command::new(env!("CARGO_BIN_EXE_surfacecheck"))
}

#[test]
fn detects_stale_readme_and_returns_one() {
    let temp = TempDir::new().expect("temp directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"2.0.0\"\n",
    )
    .expect("manifest");
    fs::write(temp.path().join("README.md"), "Demo version 1.0.0\n").expect("readme");
    fs::write(
        temp.path().join("surfacecheck.toml"),
        r#"
[project]
name = "demo"
version_source = "Cargo.toml"

[[surface]]
name = "README"
kind = "markdown"
path = "README.md"
require = ["{version}"]
"#,
    )
    .expect("config");

    let output = binary()
        .current_dir(temp.path())
        .args(["check", "--json", "--offline"])
        .output()
        .expect("run surfacecheck");

    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(report["failed"], 1);
    assert_eq!(report["findings"][1]["code"], "content.missing");
}

#[test]
fn passes_when_local_facts_match() {
    let temp = TempDir::new().expect("temp directory");
    fs::write(
        temp.path().join("package.json"),
        r#"{"name":"demo","version":"2.0.0"}"#,
    )
    .expect("manifest");
    fs::write(temp.path().join("README.md"), "Install demo 2.0.0\n").expect("readme");
    fs::write(
        temp.path().join("surfacecheck.toml"),
        r#"
[project]
name = "demo"
version_source = "package.json"

[[surface]]
name = "README"
kind = "markdown"
path = "README.md"
require = ["{version}"]
"#,
    )
    .expect("config");

    let output = binary()
        .current_dir(temp.path())
        .args(["check", "--offline"])
        .output()
        .expect("run surfacecheck");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("0 failed"));
}

#[test]
fn reads_version_from_a_cargo_workspace_manifest() {
    let temp = TempDir::new().expect("temp directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"2.0.0\"\n",
    )
    .expect("manifest");
    fs::write(temp.path().join("CHANGELOG.md"), "## v2.0.0\n").expect("changelog");
    fs::write(
        temp.path().join("surfacecheck.toml"),
        r#"
[project]
name = "demo"
version_source = "Cargo.toml"

[[surface]]
name = "Changelog"
kind = "markdown"
path = "CHANGELOG.md"
require = ["v{version}"]
"#,
    )
    .expect("config");

    let output = binary()
        .current_dir(temp.path())
        .args(["check", "--offline"])
        .output()
        .expect("run surfacecheck");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("demo 2.0.0"));
}

#[test]
fn init_creates_a_working_configuration() {
    let temp = TempDir::new().expect("temp directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.4.0\"\n",
    )
    .expect("manifest");
    fs::write(
        temp.path().join("README.md"),
        "demo 0.4.0\ncargo install demo\n",
    )
    .expect("readme");

    let init = binary()
        .current_dir(temp.path())
        .arg("init")
        .output()
        .expect("run init");
    assert!(init.status.success());

    let check = binary()
        .current_dir(temp.path())
        .args(["check", "--offline"])
        .output()
        .expect("run check");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn checks_live_llms_content_and_delivery_headers() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0_u8; 2048];
        let request_length = stream.read(&mut request).expect("read request");
        let request = String::from_utf8_lossy(&request[..request_length]).into_owned();
        let body = "# demo\nVersion: 2.0.0\n";
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .expect("write response");
        request
    });

    let temp = TempDir::new().expect("temp directory");
    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"2.0.0\"\n",
    )
    .expect("manifest");
    fs::write(
        temp.path().join("surfacecheck.toml"),
        format!(
            r#"
[project]
name = "demo"
version_source = "Cargo.toml"

[[surface]]
name = "llms.txt"
kind = "llms"
url = "http://{address}/v{{version}}/llms.txt"
require = ["{{version}}"]
"#
        ),
    )
    .expect("config");

    let output = binary()
        .current_dir(temp.path())
        .args(["check", "--json"])
        .output()
        .expect("run surfacecheck");
    let request = server.join().expect("test server");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    assert_eq!(report["failed"], 0);
    assert!(request.starts_with("GET /v2.0.0/llms.txt "));
}

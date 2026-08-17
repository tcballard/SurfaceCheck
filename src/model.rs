use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub status: Status,
    pub code: String,
    pub surface: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

impl Finding {
    pub fn pass(code: &str, surface: &str, message: impl Into<String>) -> Self {
        Self {
            status: Status::Pass,
            code: code.to_owned(),
            surface: surface.to_owned(),
            message: message.into(),
            evidence: None,
        }
    }

    pub fn fail(
        code: &str,
        surface: &str,
        message: impl Into<String>,
        evidence: Option<String>,
    ) -> Self {
        Self {
            status: Status::Fail,
            code: code.to_owned(),
            surface: surface.to_owned(),
            message: message.into(),
            evidence,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub project: String,
    pub version: String,
    pub passed: usize,
    pub failed: usize,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn new(project: String, version: String, findings: Vec<Finding>) -> Self {
        let passed = findings
            .iter()
            .filter(|finding| finding.status == Status::Pass)
            .count();
        let failed = findings.len() - passed;
        Self {
            project,
            version,
            passed,
            failed,
            findings,
        }
    }
}

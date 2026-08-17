use anyhow::Result;

use crate::model::{Report, Status};

pub fn print_human(report: &Report) {
    println!("{} {}", report.project, report.version);
    println!();
    for finding in &report.findings {
        let label = match finding.status {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
        };
        println!("{label:<4}  {:<28} {}", finding.surface, finding.message);
        if let Some(evidence) = &finding.evidence {
            println!("      evidence: {evidence}");
        }
    }
    println!();
    println!("{} passed, {} failed", report.passed, report.failed);
}

pub fn print_json(report: &Report) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}

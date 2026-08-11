//! Reviewed, pinned reductions from real open-source Ruby projects.
//!
//! This corpus is deliberately separate from the weighted inference
//! scorecard. It measures precision: concrete results must match reviewed
//! ground truth, proven mutations must report their one expected diagnostic,
//! and incomplete dynamic boundaries must remain diagnostic-free.

use std::collections::BTreeSet;
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use serde::{Deserialize, Serialize};

use crate::test::harness::{check, check_multi_file};

const CORPUS_SOURCE: &str =
    include_str!("../../support/type_inference/real_project_precision.toml");

#[derive(Debug, Deserialize)]
struct Corpus {
    schema_version: u32,
    name: String,
    corpus_revision: String,
    review_date: String,
    minimum_projects: usize,
    minimum_cases: usize,
    notes: String,
    projects: Vec<Project>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Project {
    id: String,
    repository: String,
    revision: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Expectation {
    ConcreteType,
    Diagnostic,
    DiagnosticSuppression,
    ExplainedUnknown,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    project: String,
    expectation: Expectation,
    source_path: String,
    source_lines: String,
    source_url: String,
    reduction: String,
    rationale: String,
    files: Vec<CaseFile>,
}

#[derive(Debug, Deserialize)]
struct CaseFile {
    path: String,
    fixture: String,
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    schema_version: u32,
    name: &'a str,
    corpus_revision: &'a str,
    review_date: &'a str,
    project_count: usize,
    case_count: usize,
    passed_case_count: usize,
    concrete_type_total: usize,
    concrete_type_passed: usize,
    diagnostic_total: usize,
    diagnostic_passed: usize,
    diagnostic_suppression_total: usize,
    diagnostic_suppression_passed: usize,
    explained_unknown_total: usize,
    explained_unknown_passed: usize,
    known_false_positive_count: usize,
    claim_eligible: bool,
    notes: &'a str,
    cases: Vec<CaseReport<'a>>,
}

#[derive(Debug, Serialize)]
struct CaseReport<'a> {
    id: &'a str,
    project: &'a str,
    expectation: Expectation,
    passed: bool,
    source_url: &'a str,
    failure: Option<String>,
}

fn parse_corpus() -> Corpus {
    toml::from_str(CORPUS_SOURCE).expect(
        "INVARIANT VIOLATED: the reviewed real-project precision corpus is invalid. This is a bug because release precision evidence must remain machine-readable. Fix: update support/type_inference/real_project_precision.toml and its schema together.",
    )
}

fn validate_corpus(corpus: &Corpus) {
    assert_eq!(
        corpus.schema_version, 1,
        "INVARIANT VIOLATED: the reviewed real-project precision schema is unsupported. This is a bug because provenance and result meaning cannot change implicitly. Fix: add an explicit schema migration before changing schema_version."
    );
    assert!(
        !corpus.name.trim().is_empty()
            && !corpus.corpus_revision.trim().is_empty()
            && !corpus.review_date.trim().is_empty()
            && !corpus.notes.trim().is_empty(),
        "INVARIANT VIOLATED: reviewed precision corpus metadata is incomplete. This is a bug because a release claim must identify its reviewed evidence. Fix: provide name, revision, review date, and notes."
    );

    let mut project_ids = BTreeSet::new();
    for project in &corpus.projects {
        assert!(
            project_ids.insert(project.id.as_str()),
            "INVARIANT VIOLATED: duplicate reviewed project `{}`. This is a bug because case provenance would be ambiguous. Fix: use one unique project id.",
            project.id
        );
        assert!(
            project.repository.starts_with("https://github.com/")
                && project.revision.len() == 40
                && project.revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "INVARIANT VIOLATED: reviewed project `{}` is not pinned to an exact GitHub commit. This is a bug because moving source cannot substantiate reproducible ground truth. Fix: record the canonical repository and 40-character commit SHA.",
            project.id
        );
    }
    assert!(
        corpus.projects.len() >= corpus.minimum_projects,
        "INVARIANT VIOLATED: reviewed precision corpus has {} projects, below its minimum of {}. This is a bug because one codebase cannot represent the release precision claim. Fix: retain representative pinned projects or keep the claim ineligible.",
        corpus.projects.len(),
        corpus.minimum_projects
    );
    assert!(
        corpus.cases.len() >= corpus.minimum_cases,
        "INVARIANT VIOLATED: reviewed precision corpus has {} cases, below its minimum of {}. This is a bug because sparse anecdotes cannot substantiate the release precision claim. Fix: add reviewed reductions or keep the claim ineligible.",
        corpus.cases.len(),
        corpus.minimum_cases
    );

    let mut case_ids = BTreeSet::new();
    let mut represented_projects = BTreeSet::new();
    let mut expectations = BTreeSet::new();
    for case in &corpus.cases {
        assert!(
            case_ids.insert(case.id.as_str()),
            "INVARIANT VIOLATED: duplicate reviewed precision case `{}`. This is a bug because result identity would be ambiguous. Fix: use one stable unique case id.",
            case.id
        );
        let project = corpus
            .projects
            .iter()
            .find(|project| project.id == case.project)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: reviewed precision case `{}` references unknown project `{}`. This is a bug because its upstream provenance cannot be verified. Fix: reference a declared pinned project.",
                    case.id, case.project
                )
            });
        represented_projects.insert(project.id.as_str());
        expectations.insert(expectation_rank(case.expectation));
        assert!(
            case.source_url.starts_with(&format!(
                "{}/blob/{}/",
                project.repository, project.revision
            )) && case.source_url.contains('#')
                && !case.source_path.trim().is_empty()
                && !case.source_lines.trim().is_empty()
                && !case.reduction.trim().is_empty()
                && !case.rationale.trim().is_empty(),
            "INVARIANT VIOLATED: reviewed precision case `{}` has incomplete or moving provenance. This is a bug because the reduction cannot be audited against exact upstream source. Fix: record a commit-pinned URL, source path/lines, reduction, and rationale.",
            case.id
        );
        assert!(
            !case.files.is_empty()
                && case
                    .files
                    .iter()
                    .all(|file| !file.path.trim().is_empty() && fixture_matches(case.expectation, &file.fixture)),
            "INVARIANT VIOLATED: reviewed precision case `{}` has no fixture or its assertion does not match {:?}. This is a bug because a case could pass without checking its reviewed ground truth. Fix: add the matching exact type, diagnostic, suppression, or explained-Unknown tag.",
            case.id,
            case.expectation
        );
    }
    assert_eq!(
        represented_projects.len(),
        corpus.projects.len(),
        "INVARIANT VIOLATED: at least one pinned project has no reviewed case. This is a bug because project count would overstate precision breadth. Fix: add a reviewed reduction or remove the unrepresented project."
    );
    assert!(
        expectations.contains(&expectation_rank(Expectation::ConcreteType))
            && expectations.contains(&expectation_rank(Expectation::Diagnostic))
            && expectations.contains(&expectation_rank(Expectation::DiagnosticSuppression)),
        "INVARIANT VIOLATED: reviewed precision corpus lacks an exact-type, positive-diagnostic, or suppression class. This is a bug because precision requires both proving and refusing results. Fix: retain all three evidence classes."
    );
}

const fn expectation_rank(expectation: Expectation) -> u8 {
    match expectation {
        Expectation::ConcreteType => 0,
        Expectation::Diagnostic => 1,
        Expectation::DiagnosticSuppression => 2,
        Expectation::ExplainedUnknown => 3,
    }
}

fn fixture_matches(expectation: Expectation, fixture: &str) -> bool {
    match expectation {
        Expectation::ConcreteType => {
            fixture.contains("<type ") || fixture.contains("<hint ") || fixture.contains("<hover ")
        }
        Expectation::Diagnostic => {
            (fixture.contains("<err") && !fixture.contains("<err none"))
                || (fixture.contains("<warn") && !fixture.contains("<warn none"))
        }
        Expectation::DiagnosticSuppression => {
            fixture.contains("<err none") || fixture.contains("<warn none")
        }
        Expectation::ExplainedUnknown => fixture.contains("Unknown["),
    }
}

#[test]
fn real_project_precision_manifest_is_valid() {
    validate_corpus(&parse_corpus());
}

#[tokio::test]
#[ignore = "reviewed real-project reductions start a fresh language server per case; run explicitly for release precision reporting"]
async fn report_real_project_precision() {
    let corpus = parse_corpus();
    validate_corpus(&corpus);

    let mut reports = Vec::with_capacity(corpus.cases.len());
    for case in &corpus.cases {
        let result = AssertUnwindSafe(run_case(case)).catch_unwind().await;
        reports.push(CaseReport {
            id: &case.id,
            project: &case.project,
            expectation: case.expectation,
            passed: result.is_ok(),
            source_url: &case.source_url,
            failure: result.err().map(panic_message),
        });
    }

    let passed = |expectation| {
        reports
            .iter()
            .filter(|case| case.expectation == expectation && case.passed)
            .count()
    };
    let total = |expectation| {
        reports
            .iter()
            .filter(|case| case.expectation == expectation)
            .count()
    };
    let known_false_positive_count = reports
        .iter()
        .filter(|case| case.expectation == Expectation::DiagnosticSuppression && !case.passed)
        .count();
    let all_passed = reports.iter().all(|case| case.passed);
    let report = Report {
        schema_version: corpus.schema_version,
        name: &corpus.name,
        corpus_revision: &corpus.corpus_revision,
        review_date: &corpus.review_date,
        project_count: corpus.projects.len(),
        case_count: corpus.cases.len(),
        passed_case_count: reports.iter().filter(|case| case.passed).count(),
        concrete_type_total: total(Expectation::ConcreteType),
        concrete_type_passed: passed(Expectation::ConcreteType),
        diagnostic_total: total(Expectation::Diagnostic),
        diagnostic_passed: passed(Expectation::Diagnostic),
        diagnostic_suppression_total: total(Expectation::DiagnosticSuppression),
        diagnostic_suppression_passed: passed(Expectation::DiagnosticSuppression),
        explained_unknown_total: total(Expectation::ExplainedUnknown),
        explained_unknown_passed: passed(Expectation::ExplainedUnknown),
        known_false_positive_count,
        claim_eligible: all_passed
            && known_false_positive_count == 0
            && corpus.projects.len() >= corpus.minimum_projects
            && corpus.cases.len() >= corpus.minimum_cases,
        notes: &corpus.notes,
        cases: reports,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect(
            "INVARIANT VIOLATED: reviewed real-project precision report could not be serialized. This is a bug because release evidence must be machine-readable. Fix: keep report fields serializable.",
        )
    );
    assert!(
        all_passed,
        "INVARIANT VIOLATED: at least one reviewed real-project precision case failed. This may expose a false positive or an unsupported proof, so the 9/10 claim is not eligible. Fix: inspect the JSON report and either correct the proof rule or explicitly keep the goal open."
    );
}

async fn run_case(case: &Case) {
    if case.files.len() == 1 {
        check(&case.files[0].fixture).await;
        return;
    }
    let files = case
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.fixture.as_str()))
        .collect::<Vec<_>>();
    check_multi_file(&files).await;
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "non-string panic payload".to_string()
}

//! Versioned M0 type-inference scorecard validation and reporting.
//!
//! The normal test validates the checked-in scoring contract. The ignored
//! reporter executes the relatively expensive LSP fixtures and compares their
//! outcomes with the recorded M0 baseline:
//!
//! ```text
//! cargo test inference_scorecard::report_m0_scorecard -- --ignored --nocapture
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::panic::AssertUnwindSafe;

use futures::FutureExt;
use ruby_analysis::UnknownReason;
use serde::{Deserialize, Serialize};

use crate::test::harness::{check, check_multi_file, FakeEditor};

const SCORECARD_SOURCE: &str = include_str!("../../support/type_inference/scorecard.toml");

#[derive(Debug, Deserialize)]
struct Scorecard {
    schema_version: u32,
    name: String,
    corpus_revision: String,
    score_eligible: bool,
    target_score: u32,
    critical_category_minimum: u32,
    minimum_cases_for_claim: usize,
    notes: String,
    categories: Vec<Category>,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Category {
    id: String,
    title: String,
    points: u32,
    critical: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Expectation {
    ConcreteType,
    Coverage,
    UnknownSafety,
    ProofSafety,
    Diagnostic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Baseline {
    Pass,
    Gap,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    category: String,
    points: u32,
    expectation: Expectation,
    allow_unknown: bool,
    baseline: Baseline,
    rationale: String,
    #[serde(default = "one")]
    repeat: usize,
    files: Vec<CaseFile>,
    #[serde(default)]
    edits: Vec<CaseFile>,
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
    score_eligible: bool,
    claim_eligible: bool,
    case_count: usize,
    passed_case_count: usize,
    recorded_gap_count: usize,
    unexpected_outcome_count: usize,
    minimum_cases_for_claim: usize,
    minimum_case_count_met: bool,
    target_score: u32,
    total_score: u32,
    possible_score: u32,
    target_met: bool,
    baseline_matches: bool,
    unknown_reason_schema_version: u32,
    unknown_reason_codes: Vec<&'static str>,
    diagnostic_cases: DiagnosticCaseReport,
    lifecycle_case_count: usize,
    notes: &'a str,
    categories: Vec<CategoryReport<'a>>,
    cases: Vec<CaseReport<'a>>,
}

#[derive(Debug, Serialize)]
struct CategoryReport<'a> {
    id: &'a str,
    title: &'a str,
    critical: bool,
    score: u32,
    possible: u32,
    percent: u32,
}

#[derive(Debug, Serialize)]
struct CaseReport<'a> {
    id: &'a str,
    category: &'a str,
    points: u32,
    expectation: Expectation,
    allow_unknown: bool,
    baseline: Baseline,
    passed: bool,
    baseline_matches: bool,
    rationale: &'a str,
    failure: Option<String>,
}

#[derive(Debug, Serialize)]
struct DiagnosticCaseReport {
    positive_total: usize,
    positive_passed: usize,
    conservative_suppression_total: usize,
    conservative_suppression_passed: usize,
}

const fn one() -> usize {
    1
}

fn parse_scorecard() -> Scorecard {
    toml::from_str(SCORECARD_SOURCE).expect(
        "INVARIANT VIOLATED: support/type_inference/scorecard.toml is invalid. This is a bug \
         because the conformance baseline must remain machine-readable. Fix: update the \
         manifest to match the versioned scorecard schema.",
    )
}

fn validate_scorecard(scorecard: &Scorecard) {
    assert_eq!(
        scorecard.schema_version, 1,
        "INVARIANT VIOLATED: the type inference scorecard schema is unsupported. This is a bug \
         because the reporter cannot interpret scoring changes implicitly. Fix: add explicit \
         migration support before changing schema_version."
    );
    assert_eq!(
        scorecard.target_score, 90,
        "INVARIANT VIOLATED: the 9/10 score target changed. This is a bug because the checked-in \
         scorecard contract fixes acceptance at 90/100. Fix: retain target_score = 90 or revise \
         the reviewed scorecard and reporter together."
    );
    assert_eq!(
        scorecard.critical_category_minimum, 85,
        "INVARIANT VIOLATED: the critical category floor changed. This is a bug because a high \
         aggregate must not hide a weak semantic area. Fix: retain the reviewed 85 percent floor."
    );

    let mut category_ids = BTreeSet::new();
    let mut category_points = BTreeMap::new();
    let total_category_points = scorecard
        .categories
        .iter()
        .map(|category| {
            assert!(
                category_ids.insert(category.id.as_str()),
                "INVARIANT VIOLATED: duplicate scorecard category `{}`. This is a bug because \
                 case ownership and category totals would be ambiguous. Fix: use one unique id \
                 per category.",
                category.id
            );
            assert!(
                category.critical,
                "INVARIANT VIOLATED: scorecard category `{}` is not critical. This is a bug \
                 because the reviewed scorecard requires every category to meet the floor. Fix: \
                 mark every current category critical or revise the scoring contract.",
                category.id
            );
            category_points.insert(category.id.as_str(), category.points);
            category.points
        })
        .sum::<u32>();
    assert_eq!(
        total_category_points, 100,
        "INVARIANT VIOLATED: scorecard category weights total {total_category_points}, not 100. \
         This is a bug because the 9/10 threshold would no longer be meaningful. Fix: restore \
         category weights to exactly 100 points."
    );

    let mut case_ids = BTreeSet::new();
    let mut assigned_points: BTreeMap<&str, u32> = BTreeMap::new();
    let mut has_unscored_safety_case = false;
    for case in &scorecard.cases {
        assert!(
            case_ids.insert(case.id.as_str()),
            "INVARIANT VIOLATED: duplicate scorecard case `{}`. This is a bug because baseline \
             outcomes would be ambiguous. Fix: give each semantic assertion a stable unique id.",
            case.id
        );
        assert!(
            category_points.contains_key(case.category.as_str()),
            "INVARIANT VIOLATED: scorecard case `{}` references unknown category `{}`. This is a \
             bug because its points cannot be assigned. Fix: use a declared category id.",
            case.id,
            case.category
        );
        assert!(
            !case.files.is_empty(),
            "INVARIANT VIOLATED: scorecard case `{}` has no project files. This is a bug because \
             no inference assertion can execute. Fix: add at least one fixture file.",
            case.id
        );
        assert!(
            case.repeat > 0,
            "INVARIANT VIOLATED: scorecard case `{}` has repeat = 0. This is a bug because the \
             case would receive a result without executing. Fix: use repeat >= 1.",
            case.id
        );
        assert!(
            case.files
                .iter()
                .chain(case.edits.iter())
                .any(|file| fixture_has_assertion(&file.fixture)),
            "INVARIANT VIOLATED: scorecard case `{}` has no fixture assertion. This is a bug \
             because syntax-only input could receive credit without checking a type or diagnostic. \
             Fix: add a supported check() tag to a query site.",
            case.id
        );
        if !case.edits.is_empty() {
            assert_eq!(
                case.files.len(),
                1,
                "INVARIANT VIOLATED: lifecycle scorecard case `{}` has {} initial files. This is a bug because the current lifecycle runner owns one explicit buffer. Fix: use one initial file or extend the runner with reviewed multi-file edit semantics.",
                case.id,
                case.files.len()
            );
            assert!(
                case.edits.iter().all(|edit| edit.path == case.files[0].path),
                "INVARIANT VIOLATED: lifecycle scorecard case `{}` edits a different path. This is a bug because the current runner must route every generation through one FakeEditor buffer. Fix: keep edit paths equal to the initial path or add multi-file lifecycle support.",
                case.id
            );
        }

        match case.expectation {
            Expectation::ConcreteType => {
                assert!(
                    !case.allow_unknown && case.points > 0,
                    "INVARIANT VIOLATED: concrete scorecard case `{}` permits Unknown or has no \
                     points. This is a bug because supported-site Unknown must not receive accuracy \
                     credit. Fix: set allow_unknown = false and assign reviewed points.",
                    case.id
                );
            }
            Expectation::Coverage => {
                assert!(
                    !case.allow_unknown && case.points == 0,
                    "INVARIANT VIOLATED: coverage case `{}` permits Unknown or carries score. \
                     This is a bug because supplemental breadth must validate an exact result \
                     without silently changing the reviewed 100-point weighting. Fix: set \
                     allow_unknown = false and points = 0.",
                    case.id
                );
            }
            Expectation::UnknownSafety => {
                assert!(
                    case.allow_unknown,
                    "INVARIANT VIOLATED: Unknown safety case `{}` rejects Unknown. This is a bug \
                     because its purpose is to prove fail-closed behavior. Fix: set \
                     allow_unknown = true.",
                    case.id
                );
                if case.points == 0 {
                    has_unscored_safety_case = true;
                }
            }
            Expectation::ProofSafety => {
                assert_eq!(
                    case.points, 0,
                    "INVARIANT VIOLATED: proof-safety case `{}` has accuracy points. This is a \
                     bug because conservative refusal and filtering must not inflate inference \
                     accuracy. Fix: set points = 0.",
                    case.id
                );
                has_unscored_safety_case = true;
            }
            Expectation::Diagnostic => {
                assert!(
                    !case.allow_unknown && case.points == 0,
                    "INVARIANT VIOLATED: diagnostic case `{}` permits Unknown or carries type \
                     accuracy points. This is a bug because M0 diagnostic evidence is a separate \
                     precision signal. Fix: set allow_unknown = false and points = 0.",
                    case.id
                );
            }
        }

        *assigned_points.entry(case.category.as_str()).or_default() += case.points;
    }

    for category in &scorecard.categories {
        let assigned = assigned_points
            .get(category.id.as_str())
            .copied()
            .unwrap_or(0);
        assert_eq!(
            assigned, category.points,
            "INVARIANT VIOLATED: scorecard category `{}` assigns {assigned} of {} points to \
             cases. This is a bug because unassigned or excess points distort the measured score. \
             Fix: make scored cases sum exactly to the category weight.",
            category.id, category.points
        );
    }
    assert!(
        has_unscored_safety_case,
        "INVARIANT VIOLATED: the scorecard has no zero-point Unknown safety case. This is a bug \
         because refusing an unproven type must be tested without inflating inference accuracy. \
         Fix: retain at least one explicit fail-closed case with points = 0."
    );
    assert!(
        !scorecard.score_eligible || scorecard.cases.len() >= scorecard.minimum_cases_for_claim,
        "INVARIANT VIOLATED: the scorecard is claim-eligible with only {} cases, below its \
         reviewed minimum of {}. This is a bug because a seed corpus cannot substantiate 9/10. \
         Fix: add representative reviewed cases or keep score_eligible = false.",
        scorecard.cases.len(),
        scorecard.minimum_cases_for_claim
    );
}

fn fixture_has_assertion(fixture: &str) -> bool {
    [
        "<type ",
        "<hint ",
        "<hover ",
        "<err",
        "<warn",
        "<complete ",
        "<def>",
    ]
    .iter()
    .any(|marker| fixture.contains(marker))
}

#[test]
fn scorecard_manifest_is_valid() {
    let scorecard = parse_scorecard();
    validate_scorecard(&scorecard);
}

#[tokio::test]
#[ignore = "M0 scorecard starts a fresh language server per fixture; run explicitly for baseline reporting"]
async fn report_m0_scorecard() {
    let scorecard = parse_scorecard();
    validate_scorecard(&scorecard);

    let mut case_reports = Vec::with_capacity(scorecard.cases.len());
    let mut score_by_category: BTreeMap<&str, u32> = BTreeMap::new();
    let mut baseline_matches = true;

    for case in &scorecard.cases {
        let mut failure = None;
        for _ in 0..case.repeat {
            let result = AssertUnwindSafe(run_case(case)).catch_unwind().await;
            if let Err(payload) = result {
                failure = Some(panic_message(payload));
                break;
            }
        }

        let passed = failure.is_none();
        let matches = passed == (case.baseline == Baseline::Pass);
        baseline_matches &= matches;
        if passed {
            *score_by_category.entry(case.category.as_str()).or_default() += case.points;
        }
        case_reports.push(CaseReport {
            id: &case.id,
            category: &case.category,
            points: case.points,
            expectation: case.expectation,
            allow_unknown: case.allow_unknown,
            baseline: case.baseline,
            passed,
            baseline_matches: matches,
            rationale: &case.rationale,
            failure,
        });
    }

    let category_reports = scorecard
        .categories
        .iter()
        .map(|category| {
            let score = score_by_category
                .get(category.id.as_str())
                .copied()
                .unwrap_or(0);
            CategoryReport {
                id: &category.id,
                title: &category.title,
                critical: category.critical,
                score,
                possible: category.points,
                percent: score * 100 / category.points,
            }
        })
        .collect::<Vec<_>>();
    let total_score = category_reports.iter().map(|category| category.score).sum();
    let every_critical_category_meets_floor = category_reports.iter().all(|category| {
        !category.critical || category.percent >= scorecard.critical_category_minimum
    });
    let diagnostic_cases = DiagnosticCaseReport {
        positive_total: case_reports
            .iter()
            .filter(|case| case.expectation == Expectation::Diagnostic)
            .count(),
        positive_passed: case_reports
            .iter()
            .filter(|case| case.expectation == Expectation::Diagnostic && case.passed)
            .count(),
        conservative_suppression_total: case_reports
            .iter()
            .filter(|case| case.expectation == Expectation::ProofSafety)
            .count(),
        conservative_suppression_passed: case_reports
            .iter()
            .filter(|case| case.expectation == Expectation::ProofSafety && case.passed)
            .count(),
    };
    let report = Report {
        schema_version: scorecard.schema_version,
        name: &scorecard.name,
        corpus_revision: &scorecard.corpus_revision,
        score_eligible: scorecard.score_eligible,
        claim_eligible: scorecard.score_eligible
            && scorecard.cases.len() >= scorecard.minimum_cases_for_claim
            && total_score >= scorecard.target_score
            && every_critical_category_meets_floor
            && baseline_matches,
        case_count: scorecard.cases.len(),
        passed_case_count: case_reports.iter().filter(|case| case.passed).count(),
        recorded_gap_count: case_reports
            .iter()
            .filter(|case| case.baseline == Baseline::Gap)
            .count(),
        unexpected_outcome_count: case_reports
            .iter()
            .filter(|case| !case.baseline_matches)
            .count(),
        minimum_cases_for_claim: scorecard.minimum_cases_for_claim,
        minimum_case_count_met: scorecard.cases.len() >= scorecard.minimum_cases_for_claim,
        target_score: scorecard.target_score,
        total_score,
        possible_score: scorecard
            .categories
            .iter()
            .map(|category| category.points)
            .sum(),
        target_met: total_score >= scorecard.target_score,
        baseline_matches,
        unknown_reason_schema_version: 2,
        unknown_reason_codes: UnknownReason::ALL
            .iter()
            .map(|reason| reason.code())
            .collect(),
        diagnostic_cases,
        lifecycle_case_count: scorecard
            .cases
            .iter()
            .filter(|case| !case.edits.is_empty())
            .count(),
        notes: &scorecard.notes,
        categories: category_reports,
        cases: case_reports,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect(
            "INVARIANT VIOLATED: the scorecard report could not be serialized. This is a bug \
             because M0 results must be machine-readable. Fix: keep report fields serializable."
        )
    );
    assert!(
        baseline_matches,
        "INVARIANT VIOLATED: current scorecard outcomes differ from the recorded M0 baseline. \
         This may be an improvement or regression, but it must be reviewed explicitly. Fix: \
         inspect the JSON report and update only the affected baseline entries with a semantic \
         rationale."
    );
}

async fn run_case(case: &Case) {
    if !case.edits.is_empty() {
        let initial = case.files.first().expect(
            "INVARIANT VIOLATED: validated lifecycle scorecard case lost its initial file. This is a bug because validation and execution use the same immutable manifest. Fix: retain the validated case files through execution.",
        );
        let mut editor = FakeEditor::new().await;
        editor
            .open_and_check_fixture(&initial.path, &initial.fixture)
            .await;
        for edit in &case.edits {
            editor
                .set_and_check_fixture(&edit.path, &edit.fixture)
                .await;
        }
        return;
    }
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

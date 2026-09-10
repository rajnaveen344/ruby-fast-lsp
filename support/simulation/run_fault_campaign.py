#!/usr/bin/env python3
"""Prove named simulation assertions against faults in a disposable checkout.

Inspect: python3 support/simulation/run_fault_campaign.py --inventory
Run, with the repository's Cargo build slot reserved exclusively:
    python3 support/simulation/run_fault_campaign.py --run

Only the detached worktree is mutated. The invoking checkout, index, and HEAD
are never reset, cleaned, checked out, or committed. Evidence is retained even
when a mutant escapes, fails to compile, times out, or fails the wrong assertion.
The shared target directory requires caller coordination: the process check
and campaign lock cannot prevent an unrelated Cargo command starting later.
"""

from __future__ import annotations

import argparse
import dataclasses
import datetime as dt
import difflib
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time
from typing import Any


SCOPES = (
    "src", "crates", "build.rs", "Cargo.toml", "Cargo.lock",
    "rust-toolchain.toml", "extensions", "editors", "support", "tests", "AGENTS.md",
)
EXCLUDED_PARTS = {".git", ".codex", "pages", "node_modules", "target", ".cache", "__pycache__"}
EXACT_TEST = "test::simulation::exact::exact_method_results_and_rename_edits_survive_edit_recovery"
IDENTITY_TEST = "test::simulation::build_identity::tests::seed_artifact_retains_the_exact_compiled_build_identity"
DEFINITION_ANCHOR = """    let locations = query.find_definitions_at_position(&uri, position, &content)?;
    Some(GotoDefinitionResponse::Array(locations))"""
RENAME_ANCHOR = "    let result = rename::handle_rename(lang_server, params).await;"
NIL_CALL_ANCHOR = """    pub(in crate::indexer::fact_collector) fn collect_nil_call_candidate(
        &mut self,
        node: &CallNode<'_>,
    ) {"""
GUARD_ANCHOR = """        let mut engine = analysis_engine.write();
        if engine.source_snapshot_for_path(path) != Some(source_snapshot) {
            return false;
        }
        engine
            .replace_facts_if_source_snapshot(source_snapshot, facts, ResolveMode::Deferred)
            .is_some()"""


@dataclasses.dataclass(frozen=True)
class Fault:
    id: str
    description: str
    path: str
    anchor: str
    replacement: str
    test: str
    assertion: str


FAULTS = (
    Fault(
        "definition-missing", "Discard every returned method definition.",
        "src/capabilities/definitions.rs", DEFINITION_ANCHOR,
        DEFINITION_ANCHOR.replace("let locations =", "let mut locations =").replace(
            "    Some(GotoDefinitionResponse::Array(locations))",
            "    locations.clear();\n    Some(GotoDefinitionResponse::Array(locations))"),
        EXACT_TEST, "exact observation mismatch: method definition",
    ),
    Fault(
        "definition-duplicate", "Append a duplicate of the first returned definition.",
        "src/capabilities/definitions.rs", DEFINITION_ANCHOR,
        DEFINITION_ANCHOR.replace("let locations =", "let mut locations =").replace(
            "    Some(GotoDefinitionResponse::Array(locations))",
            "    if let Some(first) = locations.first().cloned() { locations.push(first); }\n"
            "    Some(GotoDefinitionResponse::Array(locations))"),
        EXACT_TEST, "exact observation mismatch: method definition",
    ),
    Fault(
        "reference-wrong-range", "Move the first reference start one UTF-16 character to the right.",
        "src/capabilities/references.rs",
        "    query.find_references_at_position(uri, position, &content)",
        """    let mut locations = query.find_references_at_position(uri, position, &content)?;
    if let Some(first) = locations.first_mut() { first.range.start.character += 1; }
    Some(locations)""",
        EXACT_TEST, "exact observation mismatch: all method calls without declarations",
    ),
    Fault(
        "rename-missing-edit", "Drop one edit from the first destination URI in lexical order.",
        "src/handlers/request.rs", RENAME_ANCHOR,
        """    let mut result = rename::handle_rename(lang_server, params).await;
    if let Some(changes) = result.as_mut().and_then(|edit| edit.changes.as_mut()) {
        let first_uri = changes.keys().min_by(|a, b| a.as_str().cmp(b.as_str())).cloned();
        if let Some(first_uri) = first_uri {
            if let Some(edits) = changes.get_mut(&first_uri) {
                if !edits.is_empty() { edits.remove(0); }
            }
        }
    }""",
        EXACT_TEST, "exact rename edit must include every target once",
    ),
    Fault(
        "rename-unrelated-edit", "Add an edit to the unrelated Other#title declaration in the neutral fixture.",
        "src/handlers/request.rs", RENAME_ANCHOR,
        """    let mut result = rename::handle_rename(lang_server, params).await;
    if let Some(changes) = result.as_mut().and_then(|edit| edit.changes.as_mut()) {
        changes.entry(Url::parse("file:///other.rb").expect("neutral fault URI is valid"))
            .or_default().push(TextEdit::new(
                Range::new(Position::new(1, 6), Position::new(1, 11)), "label".to_string()));
    }""",
        EXACT_TEST, "exact rename edit must include every target once",
    ),
    Fault(
        "nil-warning-missing", "Return before collecting any nil-call diagnostic candidate.",
        "crates/ruby-analysis/src/indexer/fact_collector/nodes/calls/nil_call.rs",
        NIL_CALL_ANCHOR,
        NIL_CALL_ANCHOR + "\n        return;",
        "test::integration::diagnostics::nil_call::nil_call_publication_survives_save_edit_and_reopen",
        "nil-call publication must match the complete expected warning",
    ),
    Fault(
        "consumer-publication-missing", "Refresh semantic facts but suppress the dependency-open consumer publication.",
        "src/capabilities/indexing.rs",
        """                server.append_current_external_linter_diagnostics(&uri, &mut diagnostics);
                server.publish_diagnostics(uri, diagnostics).await;""",
        """                server.append_current_external_linter_diagnostics(&uri, &mut diagnostics);
                let _ = (uri, diagnostics); // Isolated fault: omit consumer publication.""",
        "test::simulation::observations::late_definition_open_clears_published_unresolved_constant",
        "opening a late definition must publish the resolved consumer diagnostics",
    ),
    Fault(
        "cold-source-guards-bypassed", "Bypass both snapshot guards and replace existing-file facts with the delayed cold result.",
        "src/indexer/file_processor.rs", GUARD_ANCHOR,
        """        let _ = source_snapshot;
        let mut engine = analysis_engine.write();
        let Some(file_id) = engine.file_id(path) else { return false; };
        engine.replace_facts(file_id, facts, ResolveMode::Deferred);
        true""",
        "test::simulation::production_schedules::real_coordinator_rejects_cold_facts_after_a_startup_edit",
        "a delayed cold commit must preserve the exact new method definition",
    ),
)


SUMMARY = re.compile(
    r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;",
    re.MULTILINE,
)
COMPILE_ERROR = re.compile(r"(?:^error(?:\[E\d+\])?:|could not compile)", re.MULTILINE)
LIBTEST_FAILURE_FOOTER = re.compile(r"^error: test failed, to rerun pass `--lib`\r?\n?\Z", re.MULTILINE)


def test_summaries(output: str) -> list[dict[str, Any]]:
    return [dict(status=status, passed=int(passed), failed=int(failed), ignored=int(ignored))
            for status, passed, failed, ignored in SUMMARY.findall(output)]


def classify_test(exit_code: int | None, output: str, *, expected_test: str | None = None,
                  assertion: str | None = None, timed_out: bool = False) -> dict[str, Any]:
    """A failed build, setup panic, zero tests, or wrong assertion is never a kill."""
    summaries = test_summaries(output)
    result: dict[str, Any] = {"summaries": summaries}

    def verdict(status: str, reason: str) -> dict[str, Any]:
        return dict(result, status=status, reason=reason)

    if timed_out:
        return verdict("invalid", "process exceeded its time bound")
    error_output = output
    if exit_code == 101 and len(summaries) == 1 and summaries[0]["status"] == "FAILED":
        # Cargo prints this exact final line after a completed failing libtest.
        # Exempt only that footer; any compiler/other Cargo error stays invalid.
        error_output = LIBTEST_FAILURE_FOOTER.sub("", error_output)
    if COMPILE_ERROR.search(error_output):
        return verdict("invalid", "compilation or Cargo invocation failed")
    if len(summaries) != 1:
        return verdict("invalid", "expected exactly one libtest summary")
    summary = summaries[0]
    executed = summary["passed"] + summary["failed"]
    if executed == 0:
        return verdict("invalid", "test filter executed zero tests")
    if expected_test is not None:
        if executed != 1 or summary["ignored"] != 0:
            return verdict("invalid", "exact named test must execute once and not be ignored")
        if not re.search(r"^test " + re.escape(expected_test) + r" \.\.\.", output, re.MULTILINE):
            return verdict("invalid", "named test did not execute")
    if assertion is None:
        if exit_code == 0 and summary["status"] == "ok" and summary["failed"] == 0:
            return verdict("passed", "baseline passed with a nonzero test count")
        return verdict("invalid", "baseline must pass before any mutation is assessed")
    if exit_code == 0 and summary["status"] == "ok" and summary["failed"] == 0:
        return verdict("escaped", "mutated production behavior passed its regression test")
    if exit_code != 101 or summary["status"] != "FAILED" or summary["failed"] != 1 or summary["passed"] != 0:
        return verdict("invalid", "expected Cargo exit 101 and exactly one failing test")
    failure_section = output.rsplit("\nfailures:\n", 1)
    if len(failure_section) != 2:
        return verdict("invalid", "missing libtest failure inventory")
    failed_names = [line.strip() for line in failure_section[1].split("\ntest result:", 1)[0].splitlines() if line.strip()]
    if failed_names != [expected_test]:
        return verdict("invalid", "unexpected failing-test inventory")
    if "INVARIANT VIOLATED" in output:
        return verdict("invalid", "an invariant/setup failure does not prove semantic detection")
    panic_sections = re.split(r"thread [^\n]* panicked at ", output)
    if len(panic_sections) != 2 or assertion not in panic_sections[1]:
        return verdict("invalid", "expected one panic at the specified semantic assertion")
    return verdict("detected", "compiled mutant failed its exact named semantic assertion")


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def safe_relative(relative: str) -> PurePosixPath:
    path = PurePosixPath(relative)
    if path.is_absolute() or not path.parts or any(part in {".", ".."} for part in path.parts) or "\\" in relative:
        raise ValueError(f"unsafe workspace-relative path: {relative!r}")
    if path.parts[0] not in SCOPES:
        raise ValueError(f"path is outside the explicit overlay scope: {relative!r}")
    return path


def eligible(relative: str, *, tracked: bool) -> bool:
    path = safe_relative(relative)
    excluded = set(path.parts) & EXCLUDED_PARTS
    if not excluded:
        return True
    # Packaged guest binaries are tracked source assets, despite the target path.
    return (tracked and excluded == {"target"} and
            path.parts[:4] == ("editors", "vscode", "vsix", "extensions") and
            path.suffix == ".wasm")


def regular_path(root: Path, relative: str, *, missing_ok: bool = False) -> Path:
    parts = safe_relative(relative).parts
    path = root
    for part in parts:
        path = path / part
        if path.is_symlink():
            raise ValueError(f"overlay symlink is not allowed: {path}")
    if not path.exists() and missing_ok:
        return path
    if not path.is_file() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f"overlay input must be a regular file within the declared root: {path}")
    return path


def source_record(root: Path, relative: str, *, tracked: bool) -> dict[str, Any] | None:
    """Identify regular inputs or tracked editor-document links without following them.

    Rust/build inputs and mutation targets remain regular files. The narrow
    document-link exception supports packaging READMEs while keeping arbitrary
    linked assets out of semantic/build source capture.
    """
    parts = safe_relative(relative).parts
    path = root
    for part in parts[:-1]:
        path = path / part
        if path.is_symlink():
            raise ValueError(f"overlay symlink ancestor is not allowed: {path}")
    path = path / parts[-1]
    if path.is_symlink():
        if not tracked:
            raise ValueError(f"untracked overlay symlink is not allowed: {path}")
        document_name = path.name.lower()
        is_editor_document = parts[0] == "editors" and any(
            document_name == stem or document_name in {stem + suffix for suffix in (".md", ".txt", ".rst", ".adoc")}
            for stem in ("readme", "license", "notice", "changelog")
        )
        if not is_editor_document or relative in {fault.path for fault in FAULTS}:
            raise ValueError(f"build/source or mutation inputs cannot be symlinks: {path}")
        link_target = os.readlink(path)
        if Path(link_target).is_absolute():
            raise ValueError(f"tracked overlay link must be checkout-relative: {path}")
        resolved = path.resolve(strict=True)
        if not resolved.is_relative_to(root.resolve()) or not resolved.is_file():
            raise ValueError(f"tracked overlay link must resolve to a file within its repository: {path}")
        return {"path": relative, "kind": "symlink", "link_target": link_target,
                "sha256": sha256(os.fsencode(link_target)), "git_mode": "120000"}
    path = regular_path(root, relative, missing_ok=True)
    if not path.exists():
        return None
    return {"path": relative, "kind": "file", "sha256": sha256(path.read_bytes()),
            "git_mode": "100755" if path.stat().st_mode & stat.S_IXUSR else "100644"}


def inventory_differences(expected: list[dict[str, Any]], actual: list[dict[str, Any]]) -> dict[str, Any]:
    expected_paths = {record["path"]: record for record in expected}
    actual_paths = {record["path"]: record for record in actual}
    return {"expected_count": len(expected), "actual_count": len(actual), "differences": [
        {"path": path, "expected": expected_paths.get(path), "actual": actual_paths.get(path)}
        for path in sorted(expected_paths.keys() | actual_paths.keys())
        if expected_paths.get(path) != actual_paths.get(path)
    ]}


def replace_once(source: bytes, anchor: str, replacement: str) -> bytes:
    before = anchor.encode("utf-8")
    count = source.count(before)
    if count != 1:
        raise ValueError(f"mutation requires exactly one source anchor, found {count}")
    return source.replace(before, replacement.encode("utf-8"), 1)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    temporary.replace(path)


def retain_driver_sources(evidence: Path) -> dict[str, Any]:
    records = []
    directory = evidence / "driver"
    directory.mkdir(parents=True, exist_ok=True)
    for name in ("run_fault_campaign.py", "test_fault_campaign.py"):
        source = Path(__file__).resolve().with_name(name)
        content = source.read_bytes()
        (directory / name).write_bytes(content)
        records.append({"path": f"driver/{name}", "sha256": sha256(content)})
    return {"source_files": records, "python_version": sys.version, "python_executable": sys.executable}


class Campaign:
    def __init__(self, root: Path, process_seconds: int, campaign_seconds: int):
        self.root = root.resolve()
        self.process_seconds = process_seconds
        self.deadline = time.monotonic() + campaign_seconds
        self.evidence_parent = self.root / "target" / "simulation-reliability"
        self.evidence_parent.mkdir(parents=True, exist_ok=True)
        stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.evidence = Path(tempfile.mkdtemp(prefix=f"fault-campaign-{stamp}-", dir=self.evidence_parent))
        self.worktree = self.evidence / "worktree"
        self.target = self.root / "target"
        self.lock = self.evidence_parent / "campaign.lock"
        self.lock_acquired = False
        self.worktree_created = False
        self.pristine: dict[str, bytes] = {}
        self.report: dict[str, Any] = {
            "schema_version": 1, "status": "running", "evidence": str(self.evidence),
            "started_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
            "commands": [], "baseline": [], "faults": [],
            "scope": list(SCOPES), "excluded_components": sorted(EXCLUDED_PARTS),
            "tracked_exception": "editors/vscode/vsix/extensions/**/target/**/*.wasm",
            "symlink_policy": "Only tracked, relative editor README/LICENSE/NOTICE/CHANGELOG document links resolving within the repository are retained as link-text identities; source/build inputs, untracked links, and symlink ancestors are rejected.",
            "mode_policy": "Inventory identity uses Git's regular-file/executable/symlink modes (100644, 100755, 120000); local owner/group write bits and symlink permissions are not portable source identity.",
            "limits_seconds": {"process": process_seconds, "campaign": campaign_seconds},
            "limitations": [
                "Eight selected production faults are a reviewed inventory, not exhaustive feature or code coverage.",
                "In-process tests do not prove editor transport delivery, supported platform behavior, or beta usage.",
                "Shared CARGO_TARGET_DIR requires exclusive caller coordination; the process check is only an admission check.",
                "Rust build.json source identity excludes editor/runtime assets; the retained overlay manifest separately identifies those inputs.",
            ],
        }
        self.report["driver"] = retain_driver_sources(self.evidence)
        (self.evidence / "REPLAY.md").write_text(
            "# Replaying retained fault evidence\n\n"
            "`report.json` identifies the baseline commit, exact scoped overlay, "
            "commands, expected assertions, outcomes, and retained build identities. "
            "`inventory.json` contains each exact original anchor and replacement. "
            "The driver and pure validator copies are hashed in the report, "
            "which also identifies the Python interpreter.\n\n"
            "To reproduce an individual result, create a new detached worktree from "
            "`baseline_head` using a new unused path. Apply `overlay.patch` with "
            "`git apply --binary`; copy the files under `overlay-untracked/` into "
            "that checkout, preserving relative paths and modes. Verify those "
            "sources against `overlay-manifest.json`. Run the baseline command "
            "from the report before applying one `faults/<id>/mutation.patch`. "
            "Run that fault's identity control and exact regression command. "
            "Use a fresh private TMPDIR/TMP/TEMP and compare the resulting "
            "build.json source and executable identities with the retained files. "
            "Reserve an exclusive Cargo target; substitute only the new checkout, "
            "target, and temporary paths in recorded commands and environments. "
            "Restore the overlay source before another fault.\n\n"
            "Do not apply fault patches to a development checkout or build shipping "
            "binaries. Only the exact semantic assertion with one failing named "
            "test and Cargo exit 101 counts as detection. A compilation failure, "
            "timeout, setup/invariant panic, or zero-test result is invalid evidence. "
            "The campaign removes its own disposable checkout, while these logs, "
            "inputs, patches, and identities remain available.\n",
            encoding="utf-8",
        )

    def save(self) -> None:
        write_json(self.evidence / "report.json", self.report)

    def command(self, args: list[str], label: str, *, cwd: Path | None = None,
                input_bytes: bytes | None = None, cleanup: bool = False) -> tuple[int | None, str, bool]:
        remaining = self.deadline - time.monotonic()
        if remaining <= 0 and not cleanup:
            raise TimeoutError("whole campaign deadline exceeded")
        timeout = min(self.process_seconds, max(0.1, remaining)) if not cleanup else 60
        directory = self.evidence / label
        directory.mkdir(parents=True, exist_ok=True)
        scratch = directory / "temp"
        scratch.mkdir(exist_ok=True)
        environment = os.environ.copy()
        for name in ("TMPDIR", "TMP", "TEMP"):
            environment[name] = str(scratch)
        environment["CARGO_TARGET_DIR"] = str(self.target)
        environment["CARGO_TERM_COLOR"] = "never"
        environment["RUST_BACKTRACE"] = "0"
        # Campaign seed selection is deterministic and explicitly retained.
        environment.pop("SIM_RANDOM_SEEDS", None)
        environment["SIM_SEED"] = "42"
        started = time.monotonic()
        entry: dict[str, Any] = {
            "label": label, "argv": args, "cwd": str(cwd or self.root),
            "log": str((directory / "output.log").relative_to(self.evidence)),
            "temporary_root": str(scratch.relative_to(self.evidence)),
            "environment_overrides": {key: environment[key] for key in
                ("TMPDIR", "TMP", "TEMP", "CARGO_TARGET_DIR", "CARGO_TERM_COLOR", "RUST_BACKTRACE", "SIM_SEED")},
        }
        self.report["commands"].append(entry)
        self.save()
        timed_out = False
        process: subprocess.Popen[bytes] | None = None
        with (directory / "output.log").open("wb") as output:
            try:
                process = subprocess.Popen(
                    args, cwd=cwd or self.root, env=environment,
                    stdin=subprocess.PIPE if input_bytes is not None else subprocess.DEVNULL,
                    stdout=output, stderr=subprocess.STDOUT, start_new_session=(os.name == "posix"),
                )
                process.communicate(input_bytes, timeout=timeout)
            except (subprocess.TimeoutExpired, KeyboardInterrupt):
                timed_out = True
                if process is not None:
                    if os.name == "posix":
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                                       stdout=output, stderr=subprocess.STDOUT, timeout=20, check=False)
                        process.kill()
                    process.wait(timeout=20)
                if sys.exc_info()[0] is KeyboardInterrupt:
                    raise
            finally:
                entry.update(exit_code=process.returncode if process else None,
                             timed_out=timed_out, elapsed_seconds=round(time.monotonic() - started, 3))
                self.save()
        text = (directory / "output.log").read_text(encoding="utf-8", errors="replace")
        return entry["exit_code"], text, timed_out

    def git_bytes(self, arguments: list[str], label: str, *, cwd: Path | None = None) -> bytes:
        code, _, timed_out = self.command(["git", *arguments], label, cwd=cwd)
        if code != 0 or timed_out:
            raise RuntimeError(f"Git operation failed; inspect {label}/output.log")
        return (self.evidence / label / "output.log").read_bytes()

    def require_pass(self, args: list[str], label: str, *, exact: str | None = None) -> dict[str, Any]:
        code, output, timed_out = self.command(args, label, cwd=self.worktree)
        outcome = classify_test(code, output, expected_test=exact, timed_out=timed_out)
        outcome["command"] = label
        self.report["baseline"].append(outcome)
        self.save()
        if outcome["status"] != "passed":
            raise RuntimeError(f"required green test failed: {label}: {outcome['reason']}")
        return outcome

    def test_command(self, test: str, *, exact: bool) -> list[str]:
        return ["cargo", "test", "--locked", "--lib", test, "--", *(["--exact"] if exact else []),
                "--nocapture", "--test-threads=1"]

    def source_inventory(self, root: Path, paths: list[str], *, tracked_paths: set[str]) -> list[dict[str, Any]]:
        records = []
        for relative in sorted(set(paths)):
            record = source_record(root, relative, tracked=relative in tracked_paths)
            if record is not None:
                records.append(record)
        return records

    def assert_inventory(self, root: Path, paths: list[str], *, tracked_paths: set[str],
                         expected: list[dict[str, Any]], label: str, message: str) -> None:
        actual = self.source_inventory(root, paths, tracked_paths=tracked_paths)
        if actual != expected:
            artifact = self.evidence / label / "inventory-mismatch.json"
            write_json(artifact, inventory_differences(expected, actual))
            raise RuntimeError(f"{message}; inspect {artifact.relative_to(self.evidence)}")

    def assert_idle(self) -> None:
        if os.name != "posix":
            raise RuntimeError("the current shared Cargo target admission check requires a POSIX host")
        code, output, timed_out = self.command(["ps", "-A", "-o", "pid=,comm="], "admission/processes")
        if code != 0 or timed_out:
            raise RuntimeError("cannot verify that the shared Cargo build slot is idle")
        active = [line.strip() for line in output.splitlines()
                  if line.strip() and Path(line.strip().split(maxsplit=1)[-1]).name in {"cargo", "rustc"}]
        if active:
            raise RuntimeError(f"shared Cargo build slot is occupied: {active}")

    def create_overlay(self) -> None:
        self.lock.mkdir()
        self.lock_acquired = True
        write_json(self.lock / "owner.json", {"pid": os.getpid(), "evidence": str(self.evidence)})
        self.assert_idle()
        actual_root = self.git_bytes(["rev-parse", "--show-toplevel"], "overlay/root").decode().strip()
        if Path(actual_root).resolve() != self.root:
            raise ValueError("--root must identify the actual repository root")
        head = self.git_bytes(["rev-parse", "HEAD"], "overlay/head").decode().strip()
        self.report["baseline_head"] = head
        tracked = self.git_bytes(["ls-files", "-z", "--", *SCOPES], "overlay/tracked").decode().split("\0")
        tracked = [p for p in tracked if p and eligible(p, tracked=True)]
        untracked = self.git_bytes(["ls-files", "--others", "--exclude-standard", "-z", "--", *SCOPES], "overlay/untracked").decode().split("\0")
        untracked = [p for p in untracked if p and eligible(p, tracked=False)]
        all_paths = tracked + untracked
        tracked_paths = set(tracked)
        original_records = self.source_inventory(self.root, all_paths, tracked_paths=tracked_paths)
        changed = self.git_bytes(["diff", "--name-only", "--no-renames", "-z", head, "--", *SCOPES], "overlay/changed").decode().split("\0")
        changed = [p for p in changed if p and eligible(p, tracked=True)]
        patch = self.git_bytes(["diff", "--binary", "--full-index", "--no-ext-diff", "--no-textconv", "--no-renames", head, "--", *changed],
                               "overlay/patch") if changed else b""
        (self.evidence / "overlay.patch").write_bytes(patch)
        for relative in untracked:
            source = regular_path(self.root, relative)
            destination = self.evidence / "overlay-untracked" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        self.assert_inventory(self.root, all_paths, tracked_paths=tracked_paths, expected=original_records,
                              label="overlay/source-capture", message="source changed while the overlay was captured; rerun with a stable source snapshot")
        write_json(self.evidence / "overlay-manifest.json", original_records)
        self.report["overlay"] = {"patch_sha256": sha256(patch), "manifest_sha256": sha256(
            (self.evidence / "overlay-manifest.json").read_bytes()), "untracked": sorted(untracked),
            "changed_tracked": sorted(changed), "scoped_file_count": len(original_records)}
        self.git_bytes(["worktree", "add", "--detach", str(self.worktree), head], "overlay/create-worktree")
        self.worktree_created = True
        if patch:
            code, _, timed_out = self.command(["git", "apply", "--binary", "--whitespace=nowarn", "-"],
                                               "overlay/apply", cwd=self.worktree, input_bytes=patch)
            if code != 0 or timed_out:
                raise RuntimeError("scoped current-source patch did not apply to the detached baseline")
        for relative in untracked:
            destination = regular_path(self.worktree, relative, missing_ok=True)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(self.evidence / "overlay-untracked" / relative, destination)
        self.assert_inventory(self.worktree, all_paths, tracked_paths=tracked_paths, expected=original_records,
                              label="overlay/worktree", message="detached worktree source differs from the captured overlay")
        self.overlay_paths = all_paths
        self.overlay_tracked_paths = tracked_paths
        self.overlay_records = original_records
        for fault in FAULTS:
            source = regular_path(self.worktree, fault.path).read_bytes()
            replace_once(source, fault.anchor, fault.replacement)
            self.pristine[fault.path] = source
        write_json(self.evidence / "inventory.json", [dataclasses.asdict(fault) for fault in FAULTS])
        self.save()

    def retain_identities(self, label: str) -> list[dict[str, Any]]:
        identities = []
        for path in sorted((self.evidence / label / "temp").rglob("build.json")):
            payload = json.loads(path.read_text(encoding="utf-8"))
            files = payload.get("source_files", [])
            if not files or len({entry["path"] for entry in files}) != len(files):
                raise RuntimeError(f"build identity has no complete unique source manifest: {path}")
            for record in files:
                actual = regular_path(self.worktree, record["path"])
                if sha256(actual.read_bytes()) != record["sha256"]:
                    raise RuntimeError(f"compiled identity does not match the tested source: {record['path']}")
            for field in ("source_sha256", "test_executable_sha256"):
                if not re.fullmatch(r"[a-f0-9]{64}", payload.get(field, "")):
                    raise RuntimeError(f"build identity is missing {field}")
            identities.append({"path": str(path.relative_to(self.evidence)),
                               "sha256": sha256(path.read_bytes()),
                               "source_sha256": payload["source_sha256"],
                               "test_executable_sha256": payload["test_executable_sha256"]})
        if not identities:
            raise RuntimeError(f"no seed build.json was retained for {label}")
        return identities

    def restore(self) -> None:
        for relative, source in self.pristine.items():
            regular_path(self.worktree, relative).write_bytes(source)

    def run(self) -> int:
        print(f"Fault campaign evidence: {self.evidence}", flush=True)
        self.save()
        try:
            self.create_overlay()
            # --lib confines this campaign to the test harness; no shipping binary is built.
            code, listing, timed_out = self.command(
                ["cargo", "test", "--locked", "--lib", "--", "--list"], "baseline/list", cwd=self.worktree)
            if code != 0 or timed_out:
                raise RuntimeError("test discovery must compile and complete successfully")
            listed = re.findall(r"^(.+): test$", listing, re.MULTILINE)
            for name in {IDENTITY_TEST, *(fault.test for fault in FAULTS)}:
                if listed.count(name) != 1:
                    raise RuntimeError(f"expected exactly one discovered test named {name}")
            self.require_pass(self.test_command("test::simulation", exact=False), "baseline/simulation")
            self.report["baseline_build_identities"] = self.retain_identities("baseline/simulation")
            for index, name in enumerate(sorted({fault.test for fault in FAULTS})):
                self.require_pass(self.test_command(name, exact=True), f"baseline/target-{index}", exact=name)
            print("Baseline simulation suite and all selected regression tests passed.", flush=True)
            for fault in FAULTS:
                self.restore()
                self.assert_inventory(self.worktree, self.overlay_paths, tracked_paths=self.overlay_tracked_paths,
                                      expected=self.overlay_records, label=f"faults/{fault.id}/pristine",
                                      message="test execution changed source outside the explicit mutation")
                outcome: dict[str, Any] = {
                    "id": fault.id, "test": fault.test, "assertion": fault.assertion,
                    "description": fault.description, "source_path": fault.path, "status": "running",
                }
                self.report["faults"].append(outcome)
                began = time.monotonic()
                try:
                    source = replace_once(self.pristine[fault.path], fault.anchor, fault.replacement)
                    regular_path(self.worktree, fault.path).write_bytes(source)
                    outcome["original_file_sha256"] = sha256(self.pristine[fault.path])
                    outcome["mutated_file_sha256"] = sha256(source)
                    mutation = "".join(difflib.unified_diff(
                        self.pristine[fault.path].decode().splitlines(keepends=True), source.decode().splitlines(keepends=True),
                        fromfile=f"a/{fault.path}", tofile=f"b/{fault.path}")).encode()
                    patch_path = self.evidence / "faults" / fault.id / "mutation.patch"
                    patch_path.parent.mkdir(parents=True, exist_ok=True)
                    patch_path.write_bytes(mutation)
                    outcome["mutation_patch_sha256"] = sha256(mutation)
                    identity_label = f"faults/{fault.id}/identity"
                    code, output, timed_out = self.command(self.test_command(IDENTITY_TEST, exact=True), identity_label, cwd=self.worktree)
                    identity_result = classify_test(code, output, expected_test=IDENTITY_TEST, timed_out=timed_out)
                    outcome["compile_and_identity"] = identity_result
                    if identity_result["status"] != "passed":
                        outcome.update(status="invalid", reason="mutant did not compile and pass the build identity control")
                    else:
                        outcome["build_identities"] = self.retain_identities(identity_label)
                        label = f"faults/{fault.id}/regression"
                        code, output, timed_out = self.command(self.test_command(fault.test, exact=True), label, cwd=self.worktree)
                        outcome.update(classify_test(code, output, expected_test=fault.test, assertion=fault.assertion, timed_out=timed_out))
                        outcome.update(exit_code=code, command=label)
                except Exception as error:
                    outcome.update(status="invalid", reason=f"{type(error).__name__}: {error}")
                finally:
                    self.restore()
                    outcome["elapsed_seconds"] = round(time.monotonic() - began, 3)
                    self.save()
                print(f"{fault.id}: {outcome['status']} — {outcome.get('reason', '')}", flush=True)
            self.report["status"] = "passed" if all(fault["status"] == "detected" for fault in self.report["faults"]) else "failed"
        except (Exception, KeyboardInterrupt) as error:
            self.report.update(status="failed", error=f"{type(error).__name__}: {error}")
            print(f"Campaign stopped: {self.report['error']}", file=sys.stderr, flush=True)
        finally:
            if self.worktree_created:
                try:
                    self.restore()
                    if self.worktree.parent != self.evidence or self.worktree.name != "worktree":
                        raise RuntimeError("refusing cleanup outside this campaign's unique worktree")
                    code, _, timed_out = self.command(["git", "worktree", "remove", "--force", str(self.worktree)],
                                                       "cleanup/worktree", cleanup=True)
                    if code != 0 or timed_out:
                        raise RuntimeError("detached worktree cleanup failed; inspect retained cleanup log")
                    self.report["worktree_removed"] = True
                except Exception as error:
                    self.report.update(status="failed", cleanup_error=f"{type(error).__name__}: {error}")
            if self.lock_acquired:
                (self.lock / "owner.json").unlink()
                self.lock.rmdir()
            self.report["finished_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
            self.report["counts"] = {status: sum(fault["status"] == status for fault in self.report["faults"])
                                     for status in ("detected", "escaped", "invalid", "running")}
            self.save()
        print(f"Campaign {self.report['status']}: {self.evidence / 'report.json'}", flush=True)
        return 0 if self.report["status"] == "passed" else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--inventory", action="store_true", help="print exact fault anchors and assertions without running tools")
    mode.add_argument("--run", action="store_true", help="run after reserving exclusive use of this repository's Cargo target")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--process-seconds", type=int, default=600)
    parser.add_argument("--campaign-seconds", type=int, default=3600)
    args = parser.parse_args()
    if args.inventory:
        print(json.dumps([dataclasses.asdict(fault) for fault in FAULTS], indent=2))
        return 0
    if not (0 < args.process_seconds <= 600 and 0 < args.campaign_seconds <= 3600):
        parser.error("process bound must be 1–600 seconds and campaign bound 1–3600 seconds")
    return Campaign(args.root, args.process_seconds, args.campaign_seconds).run()


if __name__ == "__main__":
    raise SystemExit(main())

"""Pure classifier and source-boundary controls; these never invoke Cargo/Git."""

import unittest
import json
from pathlib import Path
import tempfile

from run_fault_campaign import (
    Campaign, FAULTS, classify_test, eligible, inventory_differences, regular_path, replace_once, retain_driver_sources, safe_relative,
    sha256, source_record,
)


NAME = "test::simulation::neutral_contract"
MARKER = "exact observation mismatch: neutral feature"

# First real definition mutant output; only the absolute executable path is
# normalized. Preserve Cargo's footer, numeric thread ID, and both failure headers.
ACTUAL_DEFINITION_FAILURE = r"""    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.43s
     Running unittests src/lib.rs (/neutral/target/debug/deps/ruby_fast_lsp-96c034002f0d757e)

running 1 test
test test::simulation::exact::exact_method_results_and_rename_edits_survive_edit_recovery ...""" " " r"""
thread 'test::simulation::exact::exact_method_results_and_rename_edits_survive_edit_recovery' (45340884) panicked at src/test/simulation/exact.rs:21:5:
assertion `left == right` failed: exact observation mismatch: method definition
  left: []
 right: ["{\"uri\":\"file:///vessel.rb\",\"range\":{\"start\":{\"line\":1,\"character\":2},\"end\":{\"line\":1,\"character\":18}}}"]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
FAILED

failures:

failures:
    test::simulation::exact::exact_method_results_and_rename_edits_survive_edit_recovery

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1696 filtered out; finished in 0.22s

error: test failed, to rerun pass `--lib`
"""


def passed(count=1):
    return (f"running {count} tests\ntest {NAME} ... ok\n\n"
            f"test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n")


def failed(message=MARKER, name=NAME):
    return (f"running 1 test\ntest {name} ... \n"
            f"thread '{name}' panicked at src/neutral.rs:10:5:\nassertion `left == right` failed: {message}\n"
            "  left: []\n right: [expected]\nFAILED\n\nfailures:\n"
            f"    {name}\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n")


class ClassifierTests(unittest.TestCase):
    def classify(self, code, output, **kwargs):
        return classify_test(code, output, expected_test=NAME, assertion=MARKER, **kwargs)["status"]

    def test_nonzero_green_baseline(self):
        self.assertEqual(classify_test(0, passed(), expected_test=NAME)["status"], "passed")

    def test_zero_test_green_is_invalid(self):
        self.assertEqual(classify_test(0, passed(0))["status"], "invalid")
        self.assertEqual(self.classify(0, passed(0)), "invalid")

    def test_surviving_mutant_escapes(self):
        self.assertEqual(self.classify(0, passed()), "escaped")

    def test_setup_panic_is_not_detection(self):
        self.assertEqual(self.classify(101, failed("fixture must be writable")), "invalid")

    def test_invariant_panic_is_not_detection_even_with_marker(self):
        self.assertEqual(self.classify(101, failed("INVARIANT VIOLATED: " + MARKER)), "invalid")

    def test_compile_error_is_not_detection_even_with_marker(self):
        self.assertEqual(self.classify(101, "error[E0425]: unresolved name\n" + failed()), "invalid")

    def test_wrong_assertion_is_not_detection(self):
        self.assertEqual(self.classify(101, failed("exact observation mismatch: another feature")), "invalid")

    def test_expected_semantic_assertion_detects_fault(self):
        self.assertEqual(self.classify(101, failed()), "detected")

    def test_wrong_test_is_invalid_even_with_expected_assertion(self):
        self.assertEqual(self.classify(101, failed(name=NAME + "_other")), "invalid")

    def test_timeout_is_not_detection_even_with_expected_assertion(self):
        self.assertEqual(self.classify(101, failed(), timed_out=True), "invalid")

    def test_wrong_exit_is_invalid(self):
        self.assertEqual(self.classify(1, failed()), "invalid")

    def test_additional_panic_is_invalid(self):
        self.assertEqual(self.classify(101, "thread 'worker' panicked at setup.rs:1:1:\nsetup failed\n" + failed()), "invalid")

    def test_multiple_summaries_are_invalid(self):
        self.assertEqual(self.classify(101, passed() + failed()), "invalid")

    def test_printed_assertion_without_panic_is_invalid(self):
        self.assertEqual(self.classify(101, failed().replace(" panicked at ", " printed ")), "invalid")

    def test_real_definition_failure_with_cargo_footer_is_detection(self):
        self.assertEqual(classify_test(101, ACTUAL_DEFINITION_FAILURE, expected_test=FAULTS[0].test,
                                      assertion=FAULTS[0].assertion)["status"], "detected")

    def test_compile_error_with_the_same_footer_is_still_invalid(self):
        output = "error[E0425]: cannot find value `missing` in this scope\n" + ACTUAL_DEFINITION_FAILURE
        self.assertEqual(classify_test(101, output, expected_test=FAULTS[0].test,
                                      assertion=FAULTS[0].assertion)["status"], "invalid")

    def test_only_exact_libtest_failure_footer_is_exempt(self):
        for output in (ACTUAL_DEFINITION_FAILURE.replace("pass `--lib`", "pass `--bin server`"),
                       ACTUAL_DEFINITION_FAILURE + "error: another Cargo error\n"):
            self.assertEqual(classify_test(101, output, expected_test=FAULTS[0].test,
                                          assertion=FAULTS[0].assertion)["status"], "invalid")

    def test_failure_footer_cannot_accompany_a_green_baseline(self):
        output = passed() + "\nerror: test failed, to rerun pass `--lib`\n"
        self.assertEqual(classify_test(0, output, expected_test=NAME)["status"], "invalid")


class SourceBoundaryTests(unittest.TestCase):
    def test_inventory_has_eight_unique_faults(self):
        self.assertEqual(len(FAULTS), 8)
        self.assertEqual(len({fault.id for fault in FAULTS}), 8)
        for fault in FAULTS:
            self.assertTrue(eligible(fault.path, tracked=True))
            self.assertNotEqual(fault.anchor, fault.replacement)

    def test_exact_anchor_must_match_once(self):
        self.assertEqual(replace_once(b"abc", "b", "x"), b"axc")
        for source in (b"ac", b"abbc"):
            with self.assertRaises(ValueError):
                replace_once(source, "b", "x")

    def test_relative_input_cannot_escape_scope(self):
        for path in ("/src/a.rs", "src/../secret", "pages/index.html", "../src/a.rs", "src\\a.rs"):
            with self.assertRaises(ValueError):
                safe_relative(path)

    def test_generated_and_cache_paths_excluded(self):
        for path in ("src/.cache/value", "crates/a/target/a.rs", "editors/node_modules/a.js", "editors/.git/config"):
            self.assertFalse(eligible(path, tracked=False))
            self.assertFalse(eligible(path, tracked=True))

    def test_only_tracked_packaged_wasm_gets_target_exception(self):
        path = "editors/vscode/vsix/extensions/guest/target/wasm32-wasip1/release/guest.wasm"
        self.assertTrue(eligible(path, tracked=True))
        self.assertFalse(eligible(path, tracked=False))
        self.assertFalse(eligible(path.replace("guest.wasm", "guest.rlib"), tracked=True))

    def test_acceptance_support_and_test_sources_are_in_scope(self):
        for path in ("support/type_inference/scorecard.toml", "support/jruby/runtime.json", "tests/neutral.rs"):
            self.assertTrue(eligible(path, tracked=True))
            self.assertTrue(eligible(path, tracked=False))

    def test_cache_named_source_modules_remain_in_scope(self):
        for path in ("src/cache/mod.rs", "crates/engine/src/cache/product.rs", "tests/cache/correctness.rs"):
            self.assertTrue(eligible(path, tracked=True))
            self.assertTrue(eligible(path, tracked=False))

    def test_driver_and_validator_provenance_is_retained_exactly(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            provenance = retain_driver_sources(evidence)
            self.assertTrue(provenance["python_version"])
            self.assertTrue(provenance["python_executable"])
            self.assertEqual(len(provenance["source_files"]), 2)
            for record in provenance["source_files"]:
                captured = (evidence / record["path"]).read_bytes()
                original = Path(__file__).with_name(Path(record["path"]).name).read_bytes()
                self.assertEqual(captured, original)
                self.assertEqual(record["sha256"], sha256(captured))


class LinkBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.readme = self.root / "README.md"
        self.readme.write_text("original documentation")
        self.link = self.root / "editors" / "npm" / "package" / "README.md"
        self.link.parent.mkdir(parents=True)
        self.link.symlink_to("../../../README.md")
        self.relative = self.link.relative_to(self.root).as_posix()

    def test_tracked_document_link_identifies_text_without_target_contents(self):
        original = source_record(self.root, self.relative, tracked=True)
        self.assertEqual(original["kind"], "symlink")
        self.assertEqual(original["link_target"], "../../../README.md")
        self.assertEqual(original["sha256"], sha256(b"../../../README.md"))
        self.readme.write_text("changed target contents do not change the link identity")
        self.assertEqual(source_record(self.root, self.relative, tracked=True), original)

    def test_changed_link_text_changes_identity_even_for_the_same_target(self):
        original = source_record(self.root, self.relative, tracked=True)
        self.link.unlink()
        self.link.symlink_to("../../.././README.md")
        self.assertNotEqual(source_record(self.root, self.relative, tracked=True), original)

    def test_untracked_document_link_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "untracked"):
            source_record(self.root, self.relative, tracked=False)

    def test_escaping_document_link_is_rejected(self):
        outside = self.root.parent / (self.root.name + "-outside.md")
        outside.write_text("outside the repository")
        self.addCleanup(outside.unlink)
        self.link.unlink()
        self.link.symlink_to("../../../../" + outside.name)
        with self.assertRaisesRegex(ValueError, "within its repository"):
            source_record(self.root, self.relative, tracked=True)

    def test_absolute_document_link_is_rejected_even_if_currently_inside_root(self):
        self.link.unlink()
        self.link.symlink_to(self.readme)
        with self.assertRaisesRegex(ValueError, "checkout-relative"):
            source_record(self.root, self.relative, tracked=True)

    def test_symlink_ancestor_is_rejected(self):
        ancestor = self.root / "editors" / "alias"
        ancestor.symlink_to("npm", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "ancestor"):
            source_record(self.root, "editors/alias/package/README.md", tracked=True)

    def test_build_and_mutation_source_links_are_rejected(self):
        for relative in ("src/neutral.rs", "src/capabilities/definitions.rs", "support/type_inference/scorecard.toml",
                         "editors/vscode/vsix/ruby_file_kinds.json"):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.symlink_to(self.readme)
            with self.assertRaisesRegex(ValueError, "build/source or mutation"):
                source_record(self.root, relative, tracked=True)

    def test_regular_path_still_rejects_a_valid_tracked_document_link(self):
        with self.assertRaisesRegex(ValueError, "symlink"):
            regular_path(self.root, self.relative)

    def test_link_identity_uses_git_symlink_mode(self):
        self.assertEqual(source_record(self.root, self.relative, tracked=True)["git_mode"], "120000")


class InventoryIdentityTests(unittest.TestCase):
    def test_local_write_permissions_do_not_change_git_source_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src" / "neutral.rs"
            source.parent.mkdir()
            source.write_text("fn neutral() {}")
            source.chmod(0o644)
            original = source_record(root, "src/neutral.rs", tracked=True)
            source.chmod(0o666)
            self.assertEqual(source_record(root, "src/neutral.rs", tracked=True), original)
            source.chmod(0o600)
            self.assertEqual(source_record(root, "src/neutral.rs", tracked=True), original)
            source.chmod(0o755)
            executable = source_record(root, "src/neutral.rs", tracked=True)
            self.assertEqual(original["git_mode"], "100644")
            self.assertEqual(executable["git_mode"], "100755")
            self.assertNotEqual(executable, original)

    def test_mismatch_manifest_preserves_missing_extra_and_changed_records(self):
        expected = [{"path": "src/changed.rs", "sha256": "old"}, {"path": "src/missing.rs", "sha256": "missing"}]
        actual = [{"path": "src/changed.rs", "sha256": "new"}, {"path": "src/extra.rs", "sha256": "extra"}]
        mismatch = inventory_differences(expected, actual)
        self.assertEqual(mismatch["expected_count"], 2)
        self.assertEqual(mismatch["actual_count"], 2)
        self.assertEqual(mismatch["differences"], [
            {"path": "src/changed.rs", "expected": expected[0], "actual": actual[0]},
            {"path": "src/extra.rs", "expected": None, "actual": actual[1]},
            {"path": "src/missing.rs", "expected": expected[1], "actual": None},
        ])

    def test_identical_inventory_has_no_differences(self):
        records = [{"path": "src/same.rs", "sha256": "same"}]
        self.assertEqual(inventory_differences(records, records)["differences"], [])

    def test_failed_inventory_check_writes_actual_and_expected_before_raising(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src" / "neutral.rs"
            source.parent.mkdir()
            source.write_text("old")
            expected = [source_record(root, "src/neutral.rs", tracked=True)]
            source.write_text("new")
            campaign = Campaign.__new__(Campaign)
            campaign.evidence = root / "evidence"
            with self.assertRaisesRegex(RuntimeError, "inventory-mismatch.json"):
                campaign.assert_inventory(root, ["src/neutral.rs"], tracked_paths={"src/neutral.rs"},
                                          expected=expected, label="check", message="source differs")
            retained = json.loads((campaign.evidence / "check" / "inventory-mismatch.json").read_text())
            self.assertEqual(retained["differences"][0]["expected"], expected[0])
            self.assertEqual(retained["differences"][0]["actual"]["sha256"], sha256(b"new"))


if __name__ == "__main__":
    unittest.main()

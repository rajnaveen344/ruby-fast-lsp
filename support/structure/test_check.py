"""Boundary and lifecycle checks for the source-folder guard."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import check


def policy():
    return {
        'version': 1, 'limit': 10, 'line_limit': 1000, 'roots': ['src', 'crates'],
        'strict_roots': ['crates/analysis'], 'excluded': {},
        'legacy': {}, 'legacy_lines': {}, 'exceptions': {},
        'layering': {
            'rules': [
                {'sources': ['src/loader', 'src/utils'], 'forbidden': ['server', 'lsp']},
                {'sources': ['src/server'], 'forbidden': ['lsp']},
            ],
            'test_exemptions': {},
        },
    }


def files(count, directory='src'):
    return [f'{directory}/file_{index}.rs' for index in range(count)]


class DirectoryLimitTests(unittest.TestCase):
    def run_audit(self, paths, configuration=None, readme=''):
        return check.audit(paths, configuration or policy(), lambda _: readme)['violations']

    def test_ten_entries_pass_but_eleven_fail(self):
        self.assertEqual(self.run_audit(files(10)), [])
        self.assertIn('11 immediate entries', self.run_audit(files(11))[0])

    def test_subfolders_and_readmes_count_once_at_each_level(self):
        paths = files(8) + ['src/README.md'] + files(10, 'src/flow')
        self.assertEqual(self.run_audit(paths), [])
        failures = self.run_audit(paths + ['src/flow/extra.rs', 'src/extra.rs'])
        self.assertEqual(len(failures), 2)
        self.assertTrue(any('src/flow: 11' in failure for failure in failures))

    def test_legacy_exact_membership_rejects_same_count_replacement(self):
        config = policy()
        config['legacy']['src'] = [Path(p).name for p in files(11)]
        self.assertEqual(self.run_audit(files(11), config), [])
        failures = self.run_audit(files(10) + ['src/new.rs'], config)
        self.assertTrue(any('new entries' in failure for failure in failures))
        self.assertTrue(any('trim removed entries' in failure for failure in failures))

    def test_cleaned_legacy_folder_must_lose_its_allowance(self):
        config = policy()
        config['legacy']['src'] = [Path(p).name for p in files(11)]
        self.assertIn('obsolete legacy', self.run_audit(files(10), config)[0])
        self.assertIn('obsolete legacy', self.run_audit([], config)[0])

    def test_strict_library_cannot_acquire_a_legacy_allowance(self):
        config = policy()
        config['legacy']['crates/analysis'] = [Path(p).name for p in files(11)]
        with self.assertRaisesRegex(ValueError, 'legacy allowance is forbidden'):
            self.run_audit(files(11, 'crates/analysis'), config)

    def test_strict_library_cannot_hide_an_oversized_subtree(self):
        config = policy()
        config['excluded']['crates/analysis/src'] = 'Convenience exclusion.'
        with self.assertRaisesRegex(ValueError, 'cannot hide excluded subfolders'):
            self.run_audit(files(11, 'crates/analysis/src'), config)

    def test_excluded_data_still_counts_as_one_parent_entry(self):
        config = policy()
        config['excluded']['src/data'] = 'Bundled upstream signatures.'
        paths = files(9) + files(50, 'src/data')
        self.assertEqual(self.run_audit(paths, config), [])
        self.assertIn('11 immediate entries', self.run_audit(paths + ['src/new.rs'], config)[0])

    def test_exception_requires_readme_and_respects_its_bound(self):
        config = policy()
        reason = 'This cohesive family has no useful semantic subdivision.'
        config['exceptions']['src'] = {'max_entries': 12, 'readme_excerpt': reason}
        paths = files(10) + ['src/README.md']
        self.assertEqual(self.run_audit(paths, config, reason), [])
        self.assertIn('local README', self.run_audit(paths, config)[0])
        self.assertIn('approved bound', self.run_audit(files(12) + ['src/README.md'], config, reason)[0])
        self.assertIn('obsolete exceptions', self.run_audit(files(9) + ['src/README.md'], config, reason)[0])

    def test_unknown_policy_keys_and_unsafe_paths_fail(self):
        config = policy()
        config['typo'] = True
        with self.assertRaises(ValueError):
            check.validate_policy(config)
        for path in ['../src', '/src', 'src/../other', 'src//nested', 'C:\\src']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                check.inventory([path])

    def test_duplicate_json_keys_fail(self):
        with self.assertRaisesRegex(ValueError, 'duplicate policy key'):
            json.loads('{"limit":10,"limit":99}', object_pairs_hook=check.unique_keys)

    def test_allowance_cannot_silently_raise_global_limit(self):
        config = policy()
        config['limit'] = 20
        with self.assertRaisesRegex(ValueError, 'must remain 10'):
            check.validate_policy(config)

    def test_cli_counts_new_sources_ignores_build_output_and_observes_deletion(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            (root / '.gitignore').write_text('src/build/\n')
            (root / 'src').mkdir()
            for name in files(10):
                (root / name).write_text('')
            subprocess.run(['git', '-C', str(root), 'add', '.'], check=True)
            (root / 'src/build').mkdir()
            for name in files(20, 'src/build'):
                (root / name).write_text('')
            configuration = root / 'policy.json'
            configuration.write_text(json.dumps(policy()))
            command = [sys.executable, '-B', str(Path(check.__file__).resolve()), '--root', str(root), '--policy', str(configuration), '--json']
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            (root / 'src/new.rs').write_text('')
            result = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(result.returncode, 1)
            self.assertIn('11 immediate entries', result.stdout)
            (root / files(10)[0]).unlink()
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            configuration.write_text('{')
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 2)


def lines(count):
    return 'line\n' * count


class FileLineLimitTests(unittest.TestCase):
    def run_audit(self, contents, configuration=None):
        return check.audit(sorted(contents), configuration or policy(), contents.__getitem__)['violations']

    def test_thousand_lines_pass_but_one_more_fails(self):
        self.assertEqual(self.run_audit({'src/a.rs': lines(1000)}), [])
        self.assertIn('1001 lines exceed 1000', self.run_audit({'src/a.rs': lines(1001)})[0])

    def test_only_audited_source_files_count(self):
        contents = {'src/data.json': lines(5000), 'docs/a.rs': lines(5000), 'src/README.md': lines(5000)}
        self.assertEqual(self.run_audit(contents), [])
        config = policy()
        config['excluded']['src/vendor'] = 'Vendored upstream code.'
        self.assertEqual(self.run_audit({'src/vendor/big.rs': lines(5000)}, config), [])
        self.assertIn('src/a.py', self.run_audit({'src/a.py': lines(1001)})[0])

    def test_legacy_file_may_shrink_but_not_grow(self):
        config = policy()
        config['legacy_lines']['src/a.rs'] = 1500
        self.assertEqual(self.run_audit({'src/a.rs': lines(1500)}, config), [])
        self.assertEqual(self.run_audit({'src/a.rs': lines(1200)}, config), [])
        self.assertIn('legacy baseline 1500', self.run_audit({'src/a.rs': lines(1501)}, config)[0])

    def test_split_or_removed_legacy_file_must_lose_its_allowance(self):
        config = policy()
        config['legacy_lines']['src/a.rs'] = 1500
        self.assertIn('obsolete legacy_lines', self.run_audit({'src/a.rs': lines(1000)}, config)[0])
        self.assertIn('obsolete legacy_lines', self.run_audit({}, config)[0])

    def test_line_allowances_apply_to_strict_roots_but_must_be_oversized_sources(self):
        config = policy()
        config['legacy_lines']['crates/analysis/a.rs'] = 1200
        self.assertEqual(self.run_audit({'crates/analysis/a.rs': lines(1200)}, config), [])
        for path, count in [('src/a.rs', 1000), ('src/a.json', 1200), ('docs/a.rs', 1200), ('src/a.rs', '1200')]:
            config = policy()
            config['legacy_lines'][path] = count
            with self.subTest(path=path, count=count), self.assertRaises(ValueError):
                check.validate_policy(config)

    def test_line_limit_cannot_be_raised(self):
        config = policy()
        config['line_limit'] = 2000
        with self.assertRaisesRegex(ValueError, 'line limit must remain 1000'):
            check.validate_policy(config)


class LayeringTests(unittest.TestCase):
    def run_audit(self, contents, configuration=None):
        return check.audit(sorted(contents), configuration or policy(), contents.__getitem__)['violations']

    def test_forbidden_crate_paths_fail_with_their_line(self):
        failures = self.run_audit({'src/loader/a.rs': 'use std::fs;\nuse crate::server::Server;\n'})
        self.assertEqual(len(failures), 1)
        self.assertIn('src/loader/a.rs:2: imports crate::server', failures[0])
        self.assertIn('crate::lsp', self.run_audit({'src/server/a.rs': 'fn f() { crate::lsp::handle(); }\n'})[0])

    def test_grouped_imports_are_checked_by_their_top_level_module(self):
        failures = self.run_audit({'src/utils/a.rs': 'use crate::{\n    utils::lsp::x,\n    lsp::{a, b},\n};\n'})
        self.assertEqual(len(failures), 1)
        self.assertIn('crate::lsp', failures[0])

    def test_allowed_layers_comments_and_nested_names_pass(self):
        contents = {
            'src/loader/a.rs': 'use crate::utils::lsp::x;\n// crate::server is described here\nuse crate::environment::y;\n',
            'src/server/a.rs': 'use crate::loader::z;\n',
            'src/lsp/a.rs': 'use crate::server::Server;\n',
            'src/loader/b.py': 'crate::server\n',
        }
        self.assertEqual(self.run_audit(contents), [])

    def test_exempt_test_module_passes_until_it_stops_violating(self):
        config = policy()
        config['layering']['test_exemptions']['src/loader/tests/mod.rs'] = 'Drives the server.'
        self.assertEqual(self.run_audit({'src/loader/tests/mod.rs': 'use crate::server::S;\n'}, config), [])
        self.assertIn('obsolete layering test exemption', self.run_audit({'src/loader/tests/mod.rs': ''}, config)[0])

    def test_only_test_modules_under_a_rule_may_be_exempt(self):
        for path in ['src/loader/a.rs', 'src/lsp/tests.rs', 'src/loader/tests.py']:
            config = policy()
            config['layering']['test_exemptions'][path] = 'Reason.'
            with self.subTest(path=path), self.assertRaises(ValueError):
                check.validate_policy(config)
        config = policy()
        config['layering']['test_exemptions']['src/loader/tests.rs'] = ' '
        with self.assertRaisesRegex(ValueError, 'needs a reason'):
            check.validate_policy(config)


if __name__ == '__main__':
    unittest.main()

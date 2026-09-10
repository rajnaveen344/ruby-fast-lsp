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
        'version': 1, 'limit': 10, 'roots': ['src', 'crates'],
        'strict_roots': ['crates/analysis'], 'excluded': {},
        'legacy': {}, 'exceptions': {},
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


if __name__ == '__main__':
    unittest.main()

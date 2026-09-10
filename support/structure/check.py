"""Check semantic source-folder limits without counting local build output."""

import argparse
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path, PurePosixPath

LIMIT = 10


def relative_path(value):
    if not isinstance(value, str) or not value or '\\' in value or ':' in value:
        raise ValueError(f"expected a repository-relative path, got {value!r}")
    path = PurePosixPath(value)
    if path.is_absolute() or '..' in path.parts or str(path) != value or value == '.':
        raise ValueError(f"expected a normalized repository-relative path, got {value!r}")
    return value


def beneath(path, root):
    return path == root or path.startswith(root + '/')


def in_scope(path, policy):
    return any(beneath(path, root) for root in policy['roots']) and not any(
        beneath(path, root) for root in policy['excluded']
    )


def validate_policy(policy):
    fields = {'version', 'limit', 'roots', 'strict_roots', 'excluded', 'legacy', 'exceptions'}
    if not isinstance(policy, dict) or set(policy) != fields:
        raise ValueError('policy must contain exactly version, limit, roots, strict_roots, excluded, legacy, and exceptions')
    if type(policy['version']) is not int or policy['version'] != 1 or policy['limit'] != LIMIT:
        raise ValueError('policy version must be 1 and the source-folder limit must remain 10')
    for name in ['roots', 'strict_roots']:
        values = policy[name]
        if not isinstance(values, list) or not values or not all(isinstance(path, str) for path in values) or len(values) != len(set(values)):
            raise ValueError(f'{name} must be a nonempty list of distinct paths')
        for path in values:
            relative_path(path)
    for name in ['excluded', 'legacy', 'exceptions']:
        if not isinstance(policy[name], dict):
            raise ValueError(f'{name} must be a path-keyed object')
        for path in policy[name]:
            relative_path(path)
    for path, reason in policy['excluded'].items():
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f'{path}: an excluded data tree needs an ownership explanation')
        if not any(beneath(path, root) for root in policy['roots']) or path in policy['roots']:
            raise ValueError(f'{path}: exclusion must be a subtree of a source root')
    for path in policy['strict_roots']:
        if not in_scope(path, policy):
            raise ValueError(f'{path}: strict roots must be audited')
        if any(beneath(excluded, path) for excluded in policy['excluded']):
            raise ValueError(f'{path}: strict source trees cannot hide excluded subfolders')
    for path, entries in policy['legacy'].items():
        if not in_scope(path, policy) or any(beneath(path, root) for root in policy['strict_roots']):
            raise ValueError(f'{path}: legacy allowance is forbidden outside scope or in a strict root')
        if not isinstance(entries, list) or not all(isinstance(entry, str) for entry in entries) or len(entries) <= LIMIT or len(set(entries)) != len(entries):
            raise ValueError(f'{path}: legacy entries must be a distinct oversized entry set')
        for entry in entries:
            if '/' in relative_path(entry):
                raise ValueError(f'{path}: legacy entries must be immediate children')
    for path, exception in policy['exceptions'].items():
        if not in_scope(path, policy) or path in policy['legacy']:
            raise ValueError(f'{path}: exception must be audited and separate from legacy debt')
        if not isinstance(exception, dict) or set(exception) != {'max_entries', 'readme_excerpt'}:
            raise ValueError(f'{path}: exception needs max_entries and readme_excerpt')
        if type(exception['max_entries']) is not int or exception['max_entries'] <= LIMIT:
            raise ValueError(f'{path}: exception needs a finite entry bound above 10')
        if not isinstance(exception['readme_excerpt'], str) or not exception['readme_excerpt'].strip():
            raise ValueError(f'{path}: exception needs its local README justification')
    return policy


def inventory(files):
    directories = defaultdict(set)
    for filename in files:
        parts = PurePosixPath(relative_path(filename)).parts
        for index in range(1, len(parts)):
            directories['/'.join(parts[:index])].add(parts[index])
    return directories


def audit(files, policy, read_text):
    validate_policy(policy)
    directories = {path: entries for path, entries in inventory(files).items() if in_scope(path, policy)}
    violations = []
    for path, entries in sorted(directories.items()):
        if len(entries) <= LIMIT:
            continue
        if path in policy['exceptions']:
            exception = policy['exceptions'][path]
            if len(entries) > exception['max_entries']:
                violations.append(f"{path}: {len(entries)} entries exceed approved bound {exception['max_entries']}")
            try:
                readme = read_text(path + '/README.md')
            except (OSError, UnicodeError):
                readme = ''
            if 'README.md' not in entries or exception['readme_excerpt'] not in readme:
                violations.append(f'{path}: local README must contain the approved family justification')
        elif path in policy['legacy']:
            expected = set(policy['legacy'][path])
            added, removed = entries - expected, expected - entries
            if added:
                violations.append(f"{path}: new entries in a legacy oversized folder: {', '.join(sorted(added))}; group it before adding entries")
            if removed:
                violations.append(f"{path}: trim removed entries from the legacy baseline: {', '.join(sorted(removed))}")
        else:
            violations.append(f'{path}: {len(entries)} immediate entries exceed {LIMIT}; group by semantic responsibility')
    for name in ['legacy', 'exceptions']:
        for path in sorted(policy[name]):
            if len(directories.get(path, set())) <= LIMIT:
                violations.append(f'{path}: remove the obsolete {name} allowance')
    return {
        'audited_directories': len(directories),
        'legacy_directories': len(policy['legacy']),
        'exceptions': len(policy['exceptions']),
        'violations': violations,
    }


def unique_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate policy key: {key}')
        result[key] = value
    return result


def working_files(root):
    output = subprocess.check_output(
        ['git', '-C', str(root), 'ls-files', '--cached', '--others', '--exclude-standard', '-z'],
        text=True,
    )
    # Deleted tracked paths are absent from the worktree; new source files count.
    return sorted({name for name in output.split('\0') if name and (
        (root / name).exists() or (root / name).is_symlink()
    )})


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--policy', type=Path)
    parser.add_argument('--json', action='store_true')
    args = parser.parse_args(argv)
    root = args.root.resolve()
    policy_path = args.policy or root / 'support/structure/policy.json'
    try:
        policy = json.loads(policy_path.read_text(), object_pairs_hook=unique_keys)
        report = audit(working_files(root), policy, lambda name: (root / name).read_text())
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f'Directory audit could not run: {error}', file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(report, indent=2))
    else:
        print(f"Audited {report['audited_directories']} source folders; {report['legacy_directories']} legacy folders, {report['exceptions']} approved exceptions.")
        for violation in report['violations']:
            print(violation, file=sys.stderr)
        if not report['violations']:
            print('Directory limits passed.')
    return 1 if report['violations'] else 0


if __name__ == '__main__':
    raise SystemExit(main())

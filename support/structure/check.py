"""Check semantic source-folder and source-file limits and module layering without counting local build output."""

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path, PurePosixPath

LIMIT = 10
LINE_LIMIT = 1000
SOURCE_SUFFIXES = ('.rs', '.py', '.js', '.mjs', '.ts', '.sh', '.rb')


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
    fields = {'version', 'limit', 'line_limit', 'roots', 'strict_roots', 'excluded', 'legacy', 'legacy_lines', 'exceptions', 'layering'}
    if not isinstance(policy, dict) or set(policy) != fields:
        raise ValueError('policy must contain exactly version, limit, line_limit, roots, strict_roots, excluded, legacy, legacy_lines, exceptions, and layering')
    if type(policy['version']) is not int or policy['version'] != 1 or policy['limit'] != LIMIT:
        raise ValueError('policy version must be 1 and the source-folder limit must remain 10')
    if policy['line_limit'] != LINE_LIMIT:
        raise ValueError(f'the source-file line limit must remain {LINE_LIMIT}')
    for name in ['roots', 'strict_roots']:
        values = policy[name]
        if not isinstance(values, list) or not values or not all(isinstance(path, str) for path in values) or len(values) != len(set(values)):
            raise ValueError(f'{name} must be a nonempty list of distinct paths')
        for path in values:
            relative_path(path)
    for name in ['excluded', 'legacy', 'legacy_lines', 'exceptions']:
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
    for path, lines in policy['legacy_lines'].items():
        if not in_scope(path, policy) or not path.endswith(SOURCE_SUFFIXES):
            raise ValueError(f'{path}: line allowance must name an audited source file')
        if type(lines) is not int or lines <= LINE_LIMIT:
            raise ValueError(f'{path}: line allowance must be an oversized line count')
    for path, exception in policy['exceptions'].items():
        if not in_scope(path, policy) or path in policy['legacy']:
            raise ValueError(f'{path}: exception must be audited and separate from legacy debt')
        if not isinstance(exception, dict) or set(exception) != {'max_entries', 'readme_excerpt'}:
            raise ValueError(f'{path}: exception needs max_entries and readme_excerpt')
        if type(exception['max_entries']) is not int or exception['max_entries'] <= LIMIT:
            raise ValueError(f'{path}: exception needs a finite entry bound above 10')
        if not isinstance(exception['readme_excerpt'], str) or not exception['readme_excerpt'].strip():
            raise ValueError(f'{path}: exception needs its local README justification')
    validate_layering(policy['layering'])
    return policy


LAYER_NAME = re.compile(r'^[a-z_][a-z0-9_]*$')
CRATE_PATH = re.compile(r'\bcrate::([a-z_][a-z0-9_]*)\b')
CRATE_GROUP = re.compile(r'\bcrate::\{')


def is_test_module(path):
    parts = PurePosixPath(path).parts
    return 'tests' in parts[:-1] or parts[-1] == 'tests.rs' or parts[-1].endswith('_tests.rs')


def validate_layering(layering):
    if not isinstance(layering, dict) or set(layering) != {'rules', 'test_exemptions'}:
        raise ValueError('layering must contain exactly rules and test_exemptions')
    rules = layering['rules']
    if not isinstance(rules, list):
        raise ValueError('layering rules must be a list')
    for rule in rules:
        if not isinstance(rule, dict) or set(rule) != {'sources', 'forbidden'}:
            raise ValueError('each layering rule needs exactly sources and forbidden')
        for name in ['sources', 'forbidden']:
            values = rule[name]
            if not isinstance(values, list) or not values or not all(isinstance(value, str) for value in values) or len(values) != len(set(values)):
                raise ValueError(f'layering {name} must be a nonempty list of distinct names')
        for source in rule['sources']:
            relative_path(source)
        for module in rule['forbidden']:
            if not LAYER_NAME.match(module):
                raise ValueError(f'layering forbids crate modules by name, got {module!r}')
    exemptions = layering['test_exemptions']
    if not isinstance(exemptions, dict):
        raise ValueError('layering test_exemptions must be a path-keyed object')
    for path, reason in exemptions.items():
        relative_path(path)
        if not path.endswith('.rs') or not is_test_module(path):
            raise ValueError(f'{path}: only test modules may be exempt from layering')
        if not any(beneath(path, source) for rule in rules for source in rule['sources']):
            raise ValueError(f'{path}: layering exemption must be under a rule source')
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f'{path}: layering exemption needs a reason')


def strip_line_comment(line):
    index = line.find('//')
    return line if index < 0 else line[:index]


def group_heads(text, start):
    """Top-level module names inside a `crate::{...}` group starting at `start`."""
    heads, depth, expecting = [], 0, True
    index = start
    while index < len(text):
        char = text[index]
        if char == '{':
            depth += 1
            expecting = depth == 1
        elif char == '}':
            depth -= 1
            if depth == 0:
                break
        elif char == ',' and depth == 1:
            expecting = True
        elif depth == 1 and expecting and not char.isspace():
            match = LAYER_NAME.match(re.split(r'[^a-z0-9_]', text[index:], maxsplit=1)[0])
            if match:
                heads.append(match.group(0))
            expecting = False
        index += 1
    return heads


def crate_modules(text):
    """Crate root modules named by `crate::` paths, with their 1-based lines."""
    found = []
    lines = [strip_line_comment(line) for line in text.splitlines()]
    code = '\n'.join(lines)
    for match in CRATE_PATH.finditer(code):
        found.append((code.count('\n', 0, match.start()) + 1, match.group(1)))
    for match in CRATE_GROUP.finditer(code):
        line = code.count('\n', 0, match.start()) + 1
        found.extend((line, head) for head in group_heads(code, match.end() - 1))
    return sorted(found)


def audit_layering(sources, policy, read_text):
    layering = policy['layering']
    exemptions = layering['test_exemptions']
    violations = []
    exempt_used = set()
    for path in sources:
        if not path.endswith('.rs'):
            continue
        forbidden = {
            module
            for rule in layering['rules']
            if any(beneath(path, source) for source in rule['sources'])
            for module in rule['forbidden']
        }
        if not forbidden:
            continue
        try:
            text = read_text(path)
        except (OSError, UnicodeError):
            continue
        hits = [(line, module) for line, module in crate_modules(text) if module in forbidden]
        if not hits:
            continue
        if path in exemptions:
            exempt_used.add(path)
            continue
        for line, module in hits:
            violations.append(f'{path}:{line}: imports crate::{module}, which this layer must not depend on')
    for path in sorted(set(exemptions) - exempt_used):
        violations.append(f'{path}: remove the obsolete layering test exemption')
    return violations


def inventory(files):
    directories = defaultdict(set)
    for filename in files:
        parts = PurePosixPath(relative_path(filename)).parts
        for index in range(1, len(parts)):
            directories['/'.join(parts[:index])].add(parts[index])
    return directories


def line_count(text):
    return len(text.splitlines())


def audit_lines(sources, policy, read_text):
    violations = []
    present = set()
    for path in sources:
        try:
            lines = line_count(read_text(path))
        except UnicodeError:
            continue
        present.add(path)
        allowance = policy['legacy_lines'].get(path)
        if allowance is None:
            if lines > LINE_LIMIT:
                violations.append(f'{path}: {lines} lines exceed {LINE_LIMIT}; split by semantic responsibility')
        elif lines > allowance:
            violations.append(f'{path}: {lines} lines exceed its legacy baseline {allowance}; split it before adding code')
        elif lines <= LINE_LIMIT:
            violations.append(f'{path}: remove the obsolete legacy_lines allowance')
    for path in sorted(set(policy['legacy_lines']) - present):
        violations.append(f'{path}: remove the obsolete legacy_lines allowance')
    return violations


def audit(files, policy, read_text):
    validate_policy(policy)
    directories = {path: entries for path, entries in inventory(files).items() if in_scope(path, policy)}
    sources = sorted(path for path in files if path.endswith(SOURCE_SUFFIXES) and in_scope(path, policy))
    violations = audit_lines(sources, policy, read_text)
    violations += audit_layering(sources, policy, read_text)
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
        'audited_sources': len(sources),
        'legacy_directories': len(policy['legacy']),
        'legacy_files': len(policy['legacy_lines']),
        'exceptions': len(policy['exceptions']),
        'layering_test_exemptions': len(policy['layering']['test_exemptions']),
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
        print(f"Audited {report['audited_directories']} source folders and {report['audited_sources']} source files; {report['legacy_directories']} legacy folders, {report['legacy_files']} legacy files, {report['exceptions']} approved exceptions, {report['layering_test_exemptions']} layering test exemptions.")
        for violation in report['violations']:
            print(violation, file=sys.stderr)
        if not report['violations']:
            print('Directory, file, and layering limits passed.')
    return 1 if report['violations'] else 0


if __name__ == '__main__':
    raise SystemExit(main())

#!/usr/bin/env python3
"""Execute only the six checked-in neutral Ruby programs, with bounded children.

The ordinary Rust test validates the independent model against the same reviewed
expected targets. This runner validates those targets against actual Ruby
dispatch. Neither proves completeness of Ruby static inference.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


ROOT = Path(__file__).resolve().parents[2]
CASES = Path(__file__).with_name("oracle_cases.json")
IDS = {
    "instance-inheritance", "prepend-before-class", "include-before-superclass",
    "last-include-wins", "class-method-inheritance", "explicit-private-call-rejected",
}


def main():
    evidence_parent = ROOT / "target/simulation-reliability"
    evidence_parent.mkdir(parents=True, exist_ok=True)
    evidence = Path(tempfile.mkdtemp(prefix="oracle-controls-", dir=evidence_parent))
    source = CASES.read_bytes()
    (evidence / "oracle_cases.json").write_bytes(source)
    script = Path(__file__).read_bytes()
    (evidence / "run_oracle_controls.py").write_bytes(script)
    report = {
        "schema_version": 1, "status": "running", "cases": [],
        "cases_sha256": hashlib.sha256(source).hexdigest(),
        "runner_sha256": hashlib.sha256(script).hexdigest(),
        "limits": {"child_seconds": 10, "output_bytes": 65536},
        "limitations": ["Six neutral dispatch examples, not Ruby language completeness or type-inference coverage.",
                        "The Rust model control must also pass against this same oracle_cases.json."],
    }

    def save():
        (evidence / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    try:
        cases = json.loads(source)
        if len(cases) != len(IDS) or {case["id"] for case in cases} != IDS:
            raise ValueError("all six unique reviewed cases are required")
        executable = shutil.which(os.environ.get("SIM_RUBY", "ruby"))
        if not executable:
            raise RuntimeError("Ruby is required for the explicit oracle gate; set SIM_RUBY to its executable")
        report["selected_executable"] = executable
        environment = {key: value for key, value in os.environ.items()
                       if not (key.startswith("BUNDLE_") or key in {"RUBYOPT", "RUBYLIB", "GEM_HOME", "GEM_PATH"})}

        def run(code, name):
            stdout, stderr = evidence / (name + ".stdout"), evidence / (name + ".stderr")
            started = time.monotonic()
            command = [executable, "-e", code]
            with stdout.open("wb") as out, stderr.open("wb") as err:
                result = subprocess.run(command, cwd=evidence, env=environment, stdin=subprocess.DEVNULL,
                                        stdout=out, stderr=err, timeout=10, check=False)
            if stdout.stat().st_size > 65536 or stderr.stat().st_size > 65536:
                raise RuntimeError(f"{name}: neutral control output exceeded its bound")
            if result.returncode != 0:
                raise RuntimeError(f"{name}: Ruby failed with exit {result.returncode}; inspect retained output")
            return json.loads(stdout.read_text()), {"argv": command, "elapsed_ms": round((time.monotonic()-started)*1000),
                                                   "stdout": stdout.name, "stderr": stderr.name, "exit_code": result.returncode}

        report["runtime"], report["runtime_probe"] = run(
            "require 'json'; require 'rbconfig'; puts JSON.generate({description: RUBY_DESCRIPTION, engine: RUBY_ENGINE, executable: RbConfig.ruby})",
            "runtime")
        for case in cases:
            actual, command = run("require 'json'\nputs JSON.generate(begin\n" + case["ruby"] + "\nend)\n", case["id"])
            passed = actual == case["expected"]
            report["cases"].append({"id": case["id"], "expected": case["expected"], "actual": actual,
                                    "status": "passed" if passed else "failed", "command": command})
            save()
        report["status"] = "passed" if all(case["status"] == "passed" for case in report["cases"]) else "failed"
    except Exception as error:
        report.update(status="failed", error=f"{type(error).__name__}: {error}")
    finally:
        save()
    print(f"Neutral Ruby oracle controls {report['status']}: {evidence / 'report.json'}", flush=True)
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())

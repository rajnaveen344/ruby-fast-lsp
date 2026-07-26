# Bundled CFR Java Decompiler

Ruby Fast LSP bundles CFR 0.152 solely to provide deterministic, read-only
implementation navigation when an exact Java source attachment is unavailable.

- Upstream: <https://github.com/leibnitz27/cfr>
- Release: `0.152`
- Artifact: `cfr-0.152.jar`
- SHA-256: `f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2`
- License: MIT; see `LICENSE-CFR`

The server verifies this checksum before every decompiler process is accepted.
Decompiler output is presentation-only. JVM classfile metadata remains the
semantic authority for names, descriptors, overloads, types, visibility,
inheritance, diagnostics, completion, hover, and signature help.

# Ruby Fast LSP – Integration Test Strategy

## 1  Goals

1. Achieve **high confidence** that the server behaves correctly across the most used LSP features.
2. Provide **wide language-feature coverage** (OO, modules, mixins, metaprogramming, control-flow, etc.).
3. Keep the suite **fast** (< 30 s) so it can run on every commit in CI.
4. Produce **actionable diagnostics** on failure.

## 2  Current State

* Tests are split across dedicated modules in `src/test/`:
  * `integration_test.rs` – shared `TestHarness` utilities and a basic smoke-test
  * `definitions.rs` – all `goto definition` tests
  * `references.rs` – all `find references` tests
* Fixtures in `src/test/fixtures` (≈ 23 Ruby snippets) exercise:
  * `goto definition` for classes, methods & modules
  * `find references`
  * **Gap**: hover, completion, document symbols, workspace symbols, diagnostics, formatting, semantic tokens, rename, folding-range, etc.
* Workspace-level scenarios are covered – the harness can open every Ruby file in a fixture sub-directory to simulate a project workspace.

## 3  Coverage Matrix – Requests × Language Entities

Integration tests should exercise the **public handler API**:

* `src/handlers/request.rs` – verify that each request returns the expected `lsp_types` response.
* `src/handlers/notification.rs` – send file-change notifications (`didOpen`, `didChange`, `didSave`, `didClose`) and then assert on **index state** (e.g. definitions count, symbol positions).

The table below defines the required coverage.  A ✅ indicates that at least **one fixture** and **one assertion** must exist for that cell.

| Language Entity → \  LSP Request ↓ | Goto Definition | Hover | Completion | References | Rename | Document Symbols | Workspace Symbols | Diagnostics | Formatting | Semantic Tokens | Folding Range |
| ---------------------------------- | --------------- | ----- | ---------- | ---------- | ------ | ---------------- | ----------------- | ----------- | ---------- | --------------- | ------------- |
| Class                              | ✅               | ✅     | ✅          | ✅          | ✅      | ✅                | ✅                 | ✅           | ✅          | ✅               | ✅             |
| Module                             | ✅               | ✅     | ✅          | ✅          | ✅      | ✅                | ✅                 | ✅           | ✅          | ✅               | ✅             |
| Constant                           | ✅               | ✅     | ✅          | ✅          | ✅      | ✅                | ✅                 | ✅           | n/a        | ✅               | n/a           |
| Method (instance & class)          | ✅               | ✅     | ✅          | ✅          | ✅      | n/a              | n/a               | ✅           | ✅          | ✅               | ✅             |
| Local Variable                     | n/a             | ✅     | ✅          | ✅          | ✅      | n/a              | n/a               | ✅           | n/a        | ✅               | n/a           |
| Class Variable (`@@var`)           | n/a             | ✅     | ✅          | ✅          | ✅      | n/a              | n/a               | ✅           | n/a        | ✅               | n/a           |
| Instance Variable (`@var`)         | n/a             | ✅     | ✅          | ✅          | ✅      | n/a              | n/a               | ✅           | n/a        | ✅               | n/a           |

Notes:
* “n/a” = request is not applicable / unsupported for that entity.
* Each ✅ cell requires at least one **positive** test.  Add **negative** cases for edge scenarios (e.g. undefined constant → diagnostics error).
* For rename, ensure edits across multiple files are validated when entity is referenced in other fixtures.


## 4  Test Architecture

1. **Fixture Organisation**
   * Keep Ruby sources in `src/test/fixtures/**`.
   * Group by feature → sub-dirs: `definitions/`, `hover/`, `completion/`, `diagnostics/`, etc.
   * Each scenario gets its own folder when multiple files are required, e.g.
     ```
     fixtures/
       definitions/
         cross_file/
           a.rb
           b.rb
       rename/
         class_rename/
           before.rb
           expected_edits.json
     ```

2. **Helper API** (extend existing helpers)
   * `TestHarness` struct encapsulating:
     * `RubyLanguageServer` instance
     * helper to open *all* files in a fixture sub-dir (workspace scenario)
     * `request_*` wrapper methods with automatic assertion helpers
   * Macro `assert_goto!(file, line, char, exp_file, exp_line)` etc. for brevity.

3. **Parameterized Tests**
   * Use `insta` snapshots (e.g. `insta::assert_json_snapshot!`) together with small helper macros to apply the same test logic across many fixtures.
   * Descriptive JSON per fixture: what request to send and expected result. Example:
     ```jsonc
     {
       "request": "definition",
       "cursor": {"file": "main.rb", "line": 10, "char": 5},
       "expect": {"file": "main.rb", "line": 2}
     }
     ```

4. **Coverage Measurement**
   * Enable `cargo tarpaulin --tests --skip-clean` in CI to enforce ≥ 85 % coverage for test folder & LSP handlers.

5. **Negative & Edge Cases**
   * Files with syntax errors – ensure graceful failure + diagnostic.
   * Deep nesting (> 10 levels) & singleton classes.
   * Metaprogramming (`define_method`, `class_eval`).
   * Large files (> 2 k LOC) to test performance & token limits.

6. **Performance Guardrails**
   * Add benchmark-style tests with `cargo criterion` (optional CI job) to freeze response latency budgets.

## 5  Continuous Integration

* **GitHub Actions** workflow `ci.yml`:
  1. Cache cargo.
  2. `cargo test --all-features --workspace`.
  3. `cargo tarpaulin` – post coverage to PR comment.
  4. On macOS & Linux.
* Fail build when:
  * any integration test fails
  * coverage < threshold

## 6  Implementation Steps

1. Create folder structure under `fixtures/` (see 4.1).
2. Build `TestHarness` helpers in `src/test/mod.rs`.
3. Move existing tests into feature-grouped modules.
4. Add missing capability tests incrementally:
   * Hover & Completion (baseline)
   * Diagnostics (syntax errors)
   * Symbols & Semantic tokens
   * Rename & Formatting
5. Integrate parameterized test generator.
6. Add CI coverage job.

## 7  Maintenance

* Every new LSP feature **MUST** include at least one integration fixture & test.
* Keep helper API stable; prefer extending over ad-hoc logic in tests.
* Review slowest tests quarterly – keep total runtime acceptable.

---

### Appendix A – Useful crates

* `tower-lsp` test utilities
* `insta` for quick diffing of JSON responses
* `serde_json` for fixture descriptors

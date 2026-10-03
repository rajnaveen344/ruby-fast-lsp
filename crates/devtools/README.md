# devtools

Developer tools for Ruby Fast LSP. Nothing here ships to users; the server
crate holds only the server. Run any tool from the repository root with
`cargo run -p devtools --bin <name> -- <args>`.

| Binary                       | Job                                                                   |
| ---------------------------- | --------------------------------------------------------------------- |
| `profiler`                   | Indexing, inference, and editor-operation timing; production budgets  |
| `ast`                        | Print the Prism AST for a snippet, file, or stdin (`--loc` for spans) |
| `extension`                  | Validate or smoke-test an extension package                           |
| `bench_references`           | Reference indexing timing over a named corpus                         |
| `profile_indexer`            | DHAT heap profile of workspace indexing                               |
| `profile_file_open`          | DHAT heap profile of open, close, and reopen of one file              |
| `profile_single_file`        | Current-file analysis time for one file                               |
| `profile_project_collection` | Project file collection and fact-collection timing                    |

The library holds what these share: `corpus` (pinned fixture corpora; fetch with
`snapshot.sh`), `metrics` (latency summaries and production budgets), and
`file_open`. Tools reach the server only through its public API. The
`memory-profiling` feature swaps the profiler's jemalloc allocator for DHAT;
see the [performance workflow](../../docs/development/performance.md).

//! AST explorer — dumps Prism tree for a Ruby snippet or file.
//!
//! Usage:
//!   cargo run --bin ast -- 'foo.bar&.baz'
//!   cargo run --bin ast -- --file path/to.rb
//!   cargo run --bin ast -- --stdin
//!   cargo run --bin ast -- --no-source 'x + 1'   # omit source snippets
//!   cargo run --bin ast -- --loc 'x.foo'           # include byte offsets + 1-based line:col

use ruby_prism::{parse, Node, Visit};
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut show_source = true;
    let mut show_loc = false;
    let mut from_file: Option<String> = None;
    let mut from_stdin = false;
    let mut positional: Option<String> = None;

    let mut iter = args.into_iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "--no-source" => show_source = false,
            "--loc" => show_loc = true,
            "--stdin" => from_stdin = true,
            "--file" | "-f" => from_file = iter.next(),
            "--help" | "-h" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            _ => {
                if a.starts_with("--") {
                    eprintln!("unknown flag: {}", a);
                    return ExitCode::from(2);
                }
                positional = Some(a);
            }
        }
    }

    let source: Vec<u8> = if from_stdin {
        let mut buf = Vec::new();
        if io::stdin().read_to_end(&mut buf).is_err() {
            return ExitCode::from(2);
        }
        buf
    } else if let Some(path) = from_file {
        match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("read {}: {}", path, e);
                return ExitCode::from(2);
            }
        }
    } else if let Some(s) = positional {
        s.into_bytes()
    } else {
        print_help();
        return ExitCode::from(2);
    };

    let result = parse(&source);
    let line_starts = compute_line_starts(&source);
    let mut dumper = Dumper {
        src: &source,
        depth: 0,
        show_source,
        show_loc,
        line_starts,
    };
    dumper.visit(&result.node());

    for err in result.errors() {
        eprintln!(
            "parse error: {} at {}..{}",
            err.message(),
            err.location().start_offset(),
            err.location().end_offset()
        );
    }

    ExitCode::SUCCESS
}

fn print_help() {
    eprintln!(
        "ast — dump Prism AST for Ruby source\n\n\
         Usage:\n  \
         ast 'ruby source'\n  \
         ast --file path.rb\n  \
         ast --stdin\n\n\
         Flags:\n  \
         --no-source   omit source snippets\n  \
         --loc         include byte offsets and 1-based line:col\n  \
         -h, --help    show help"
    );
}

struct Dumper<'a> {
    src: &'a [u8],
    depth: usize,
    show_source: bool,
    show_loc: bool,
    line_starts: Vec<usize>,
}

impl Dumper<'_> {
    fn print_indent(&self) {
        print!("{}", "  ".repeat(self.depth));
    }

    fn snippet(&self, start: usize, end: usize) -> String {
        let bytes = &self.src[start..end.min(self.src.len())];
        let s = String::from_utf8_lossy(bytes);
        let oneline: String = s.chars().map(|c| if c == '\n' { '⏎' } else { c }).collect();
        if oneline.chars().count() > 50 {
            let head: String = oneline.chars().take(47).collect();
            format!("{head}...")
        } else {
            oneline.to_string()
        }
    }

    fn node_kind(node: &Node) -> String {
        let dbg = format!("{node:?}");
        let end = dbg
            .find(|c: char| c == '(' || c == '{' || c == ' ')
            .unwrap_or(dbg.len());
        let raw = &dbg[..end];
        let trimmed = raw.strip_suffix("Node").unwrap_or(raw);
        to_snake(trimmed)
    }

    fn line_col(&self, offset: usize) -> (usize, usize) {
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let col = offset - self.line_starts[line_idx];
        (line_idx + 1, col)
    }

    fn fmt_loc(&self, start: usize, end: usize) -> String {
        let (sl, sc) = self.line_col(start);
        let (el, ec) = self.line_col(end);
        if sl == el {
            format!(" @ {sl}:{sc}..{ec} [{start}..{end}]")
        } else {
            format!(" @ {sl}:{sc}..{el}:{ec} [{start}..{end}]")
        }
    }

    fn print_header(&self, node: &Node, is_leaf: bool) {
        self.print_indent();
        let kind = Self::node_kind(node);
        let loc = node.location();
        let start = loc.start_offset();
        let end = loc.end_offset();
        let loc_str = if self.show_loc {
            self.fmt_loc(start, end)
        } else {
            String::new()
        };
        let snip_str = if self.show_source {
            format!(" `{}`", self.snippet(start, end))
        } else {
            String::new()
        };
        if is_leaf {
            println!("({kind}{loc_str}{snip_str})");
        } else {
            println!("({kind}{loc_str}{snip_str}");
        }
    }
}

impl<'pr> Visit<'pr> for Dumper<'_> {
    fn visit_branch_node_enter(&mut self, node: Node<'pr>) {
        self.print_header(&node, false);
        self.depth += 1;
    }

    fn visit_branch_node_leave(&mut self) {
        self.depth -= 1;
        self.print_indent();
        println!(")");
    }

    fn visit_leaf_node_enter(&mut self, node: Node<'pr>) {
        self.print_header(&node, true);
    }
}

fn compute_line_starts(src: &[u8]) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in src.iter().enumerate() {
        if *b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

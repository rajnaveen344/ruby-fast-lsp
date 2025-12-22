# lsp-repl

A generic LSP debugging REPL that works with any language server.

## Features

- **Zero-config**: Discovers custom commands from the server at runtime via `$/listCommands`
- **Standard LSP commands**: Built-in support for hover, go-to-definition, references, completions, etc.
- **Server-specific commands**: Automatically registers custom debug commands advertised by the server
- **Multi-file support**: Open multiple files and specify which file to operate on
- **Interactive REPL**: Command history, tab completion, and help

## Installation

```bash
cargo install lsp-repl
```

## Usage

```bash
# Connect to any LSP server
lsp-repl "ruby-fast-lsp --stdio"
lsp-repl "rust-analyzer"
lsp-repl "typescript-language-server --stdio"

# With a specific workspace
lsp-repl --workspace /path/to/project "ruby-fast-lsp --stdio"
```

## Commands

### File Management

```
open <file>       Open a file for analysis (alias: o)
close [file]      Close a file (current if not specified)
files             List all open files (alias: ls)
switch <file>     Switch to an open file (alias: sw)
file              Show current file info (alias: f)
```

### Position Commands

All position commands support specifying a filename:

```
# Use current file
hover 45 10

# Specify file at end
hover 45 10 user.rb

# Specify file at beginning
hover user.rb 45 10
```

Commands:

```
hover <line> <col> [file]      Get hover info (alias: h)
def <line> <col> [file]        Go to definition (alias: d)
refs <line> <col> [file]       Find references (alias: r)
complete <line> <col> [file]   Get completions (alias: c)
symbols [file]                 Document symbols (alias: s)
```

### Other

```
wait [timeout]    Wait for indexing to complete (alias: w)
help              Show help (alias: ?)
quit              Exit (alias: q, exit)
```

## Example Session

```
$ lsp-repl --workspace ~/myapp "ruby-fast-lsp --stdio"

Connecting to: ruby-fast-lsp --stdio
Workspace: /Users/me/myapp
Connected to: ruby-fast-lsp
Discovered 4 custom command(s) from server.
Type 'help' for available commands, 'quit' to exit.

> open app/models/user.rb
Opened: user.rb (150 lines)

user.rb> open app/controllers/users_controller.rb
Opened: users_controller.rb (200 lines)

users_controller.rb> files
Open files (2):
  user.rb (150 lines)
  users_controller.rb (200 lines) *

(* = current file)

users_controller.rb> hover 45 10
def show
  @user: User

# Hover on a different file without switching
users_controller.rb> hover user.rb 25 8
def find(id): User | nil

# Or specify file at the end
users_controller.rb> hover 25 8 user.rb
def find(id): User | nil

# Switch current file
users_controller.rb> switch user.rb
Switched to: user.rb

user.rb> symbols
Symbols:
  Class User (line 1)
    Method find (line 25)
    Method save (line 50)
```

## Server-Specific Commands

If the server implements `$/listCommands`, custom commands are automatically discovered:

```
> help
...

Server Commands (ruby-fast-lsp):
  lookup <fqn>       Query index for a fully qualified name
  stats              Show index statistics
  ancestors <class>  Show inheritance and mixin chain
  methods <class>    List all methods for a class

> lookup User#find
{
  "found": true,
  "entries": [{
    "fqn": "User#find",
    "kind": "Method(Instance)",
    "visibility": "Public",
    "return_type": "User | NilClass"
  }]
}
```

## The `$/listCommands` Protocol

Language servers can advertise custom commands by implementing the `$/listCommands` method:

**Request:**

```json
{ "jsonrpc": "2.0", "id": 1, "method": "$/listCommands" }
```

**Response:**

```json
{
  "commands": [
    {
      "name": "lookup",
      "method": "my-lsp/debug/lookup",
      "description": "Query index for a fully qualified name",
      "params": [{ "name": "fqn", "type": "string", "required": true }]
    }
  ]
}
```

## License

MIT

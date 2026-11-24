# Language diagnostics

RAVENBOT can ask a real Language Server Protocol (LSP) server for errors and
warnings after an agent edits code. The server is **not bundled**: RAVENBOT
detects a server already installed on `PATH`, starts it in the workspace, opens
the files the agent touched, and reads `textDocument/publishDiagnostics`.

## Where diagnostics appear

- **Agent tool** — `diagnostics` is exposed when a configured server is
  installed for the workspace and the agent has the `Shell` capability. It
  accepts optional `paths`; with no paths it checks the files the current run
  changed. The result is a normal tool payload with errors/warnings and
  1-based line/column positions.
- **1:1 goal review** — after each goal round, RAVENBOT checks the files that
  round changed. Error diagnostics are added to the round's failures, so the
  completion judge cannot mark the goal done while type errors remain. Warnings
  do not block. A missing server or a timeout is not treated as a failure: the
  deterministic gates still decide completion.
- **Projects viewer** — the `Activity` button in the file toolbar runs
  diagnostics for the open file and shows the server, file, line/column,
  severity, message, and source. The folder is validated against the app's
  configured project folders before a server is started.
- **Office agents** — the same tool is available to office agents. Office
  completion remains driven by the board, quality gates, and test/build
  evidence; office review does not run LSP automatically yet.

## Defaults

| Language | Command | Extensions |
|---|---|---|
| Rust | `rust-analyzer` | `.rs` |
| Python | `pyright-langserver --stdio` | `.py`, `.pyi` |
| TypeScript/JavaScript | `typescript-language-server --stdio` | `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs` |
| Go | `gopls` | `.go` |
| C | `clangd` | `.c`, `.h` |
| C++ | `clangd` | `.cc`, `.cpp`, `.cxx`, `.hh`, `.hpp` |

A server is selected by the extensions of the files being checked. Only
installed commands are used; a project with no matching installed server simply
has no diagnostics tool and no automatic LSP gate.

## Configuration

Open **Settings → Diagnostics → Language servers**. The editor contains the
effective JSON map and saves it to the `lsp_servers` app setting. Save `{}` to
restore the defaults.

Example override:

```json
{
  "rust": {
    "command": "/home/me/.cargo/bin/rust-analyzer",
    "args": [],
    "language_id": "rust",
    "extensions": ["rs"]
  },
  "python": {
    "command": "pylsp",
    "args": [],
    "language_id": "python",
    "extensions": ["py", "pyi"]
  }
}
```

Each entry requires a non-empty `command`, `language_id`, and at least one
extension. `args` and `initializationOptions` are passed through to the server.
The command is admin configuration, never model input.

## Timeout and safety

- `RAVENBOT_LSP_TIMEOUT_SECS` controls the per-check deadline (default 12s,
  clamped to 3–60s). Large projects may need a higher value for the first
  index.
- Running a language server is shell-grade: `rust-analyzer` may invoke Cargo
  and project build scripts. The agent tool therefore requires the `Shell`
  capability. The Settings/Projects surface is a local user action, but still
  starts the server in a configured project folder only.
- Read-only remote viewers cannot start a language server from the Projects
  button; running the project's build tooling needs an operator/owner session.
- RAVENBOT implements diagnostics only. It does not expose hover, completion,
  rename, or other mutating LSP features.
- Diagnostics reflect files on disk. A pending editor draft in RAVENBOT is not
  visible to the server until it is saved.

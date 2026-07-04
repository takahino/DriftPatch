# DriftPatch

**DriftPatch** is a desktop tool for creating, managing, and applying source-code patches. It generates token-aware `.dpatch` files (JSON) from before/after text, stores them in a patch repository, and applies them to target files — either interactively via a GUI or in bulk via a CLI.

> Japanese documentation: [README.jp.md](README.jp.md)

## Features

- **GUI editor** — Three-column layout: original (read-only), editable working copy, and patch preview
- **Token-aware patching** — Language profiles for Java, Python, C/C++, SQL, JavaScript/TypeScript, Rust, C#, Go, PL/SQL, and a generic fallback
- **Encoding support** — Automatic encoding detection on read; writes back in the original encoding
- **Patch repository** — Organized storage under `patches/<target_file>/`
- **Batch CLI** — Apply all patches at once with Excel and HTML reports
- **Git commit import** — Generate `.dpatch` files from a Git commit in bulk (read-only; no Git write operations)
- **No Git write operations** — Does not commit, push, etc. Uses libgit2 for read-only history access

## Requirements

- [Rust](https://www.rust-lang.org/) toolchain (edition 2021)
- A desktop environment supported by eframe/egui (Windows, Linux with X11/Wayland)
- [CMake](https://cmake.org/) when using Git commit import (required to build libgit2)

On Windows, the GUI loads a Japanese font automatically from system fonts (Yu Gothic, MS Gothic, or Meiryo).

## Build and Run

```bash
# Build both binaries (release)
cargo build --release

# GUI
cargo run --release
# or
./target/release/driftpatch

# Batch CLI
cargo run --release --bin driftpatch-batch -- apply \
  --workdir /path/to/project \
  --patch-dir /path/to/repo/patches \
  --report-dir /path/to/reports
```

## GUI Usage

### First-time setup

1. Launch `driftpatch`.
2. Click **Settings** (gear icon).
3. Configure:

| Setting | Description |
|---------|-------------|
| **Username** | Author name recorded in generated patches |
| **Patch repository path** | Root directory of the patch repo (contains a `patches/` folder) |
| **Work directory** | Base directory for target files; paths in patches are relative to this |
| **Git repository path** | For Git commit import; defaults to work directory if empty |

4. Click **Save and close**.

Settings are saved to:

- **Windows:** `%APPDATA%\DriftPatch\settings.json`
- **Linux:** `~/.local/share/DriftPatch/settings.json`
- **macOS:** `~/Library/Application Support/DriftPatch/settings.json`

### Workflow

```mermaid
flowchart LR
    openFile[Open file] --> edit[Edit in center column]
    edit --> generate[Generate patch]
    generate --> save[Save to patches dir]
    save --> select[Select patch in list]
    select --> preview[Preview in right column]
    preview --> apply[Apply patch]
```

1. **Open a file** — Click **Open file** and choose a source file under `work_dir`.
2. **Edit** — Modify the text in the center column (**Editable**). The left column shows the original; removed lines are highlighted in red, added lines in green.
3. **Generate a patch** — Click **Generate patch...**, enter a description (e.g. requirement ID), and click **Generate**. The patch is saved to `patches/<target_file>/<id>.dpatch`.
4. **Preview** — Select a patch in the bottom panel. The right column shows the result of applying it to the original text.
5. **Apply** — Click **Apply** to commit the selected patch to the original and editable text in memory.
6. **Delete** — Click **Delete** to remove the selected patch file from the repository.

### Import patches from a Git commit

1. In **Settings**, configure **Git repository path** (defaults to work directory), **Work directory**, and **Patch repository path**.
2. Click **Import from Git commit** in the toolbar.
3. Select a commit from the list or enter a SHA / ref manually.
4. Optionally override the description, then click **Generate**.
5. A `.dpatch` is created for every changed file in the commit. Multiple edits in the same file are split into separate patch files per hunk (`-h1`, `-h2`, etc.).

### Batch apply / conflict check from the GUI

Click **Batch apply** in the toolbar to run the same operations as `driftpatch-batch` without leaving the GUI:

1. Confirm or edit **Work directory**, **Patch directory**, and **Report directory** (pre-filled from Settings).
2. Leave **Dry run** checked to preview what each patch would do without changing any file, or uncheck it to apply for real (a warning banner appears when dry run is disabled).
3. Click **Run dry-run** / **Apply** to execute; results (summary counts and per-patch status) are shown below, along with the generated Excel/HTML report paths.
4. Click **Check conflicts** to run the same conflict check as `driftpatch-batch check` against the configured patch directory.

A real (non-dry-run) apply reloads the patch list and, if a file is currently open, re-reads it from disk so the editor reflects the applied changes.

### Three-column layout

| Column | Label | Purpose |
|--------|-------|---------|
| Left | Original (read-only) | Baseline text before changes |
| Center | Editable | Working copy for creating patches |
| Right | Patch preview (read-only) | Result of applying the selected patch |

The left and right columns scroll in sync with the center column.

### Patch list panel

The bottom panel lists patches whose `target_file` matches the currently open file. Use **Refresh** to reload from disk.

### Search in the editor

Press **Ctrl+F** (⌘F on macOS) while the center column has focus to open the search bar. Type to search the editable text; matches are highlighted (current match in orange, others in yellow) and the left/right columns scroll in sync since they follow the center column's scroll position.

| Key | Action |
|-----|--------|
| `Ctrl+F` | Open search |
| `Enter` / `F3` | Jump to next match |
| `Shift+Enter` / `Shift+F3` | Jump to previous match |
| `Esc` | Close search |

The "Aa" checkbox toggles case sensitivity (case-insensitive matching is ASCII-only).

### Opening files: drag & drop and recent files

Drag a file from Explorer/Finder onto the DriftPatch window to open it (a "Drop to open" overlay appears while dragging). The **Recent files** menu in the toolbar lists up to 10 most recently opened files (most recent first); click an entry to reopen it, or **Clear history** to empty the list. If a recent file no longer exists on disk, opening it removes it from the history and shows an error instead.

## Batch CLI Usage

`driftpatch-batch` applies every `.dpatch` file under `--patch-dir` to files in `--workdir`.

```bash
driftpatch-batch apply \
  --workdir C:\project\src \
  --patch-dir C:\project\patch-repo\patches \
  --report-dir C:\project\reports
```

| Option | Description |
|--------|-------------|
| `--workdir` | Root directory containing target source files |
| `--patch-dir` | Directory containing `.dpatch` files (typically `patches/` or the repo root) |
| `--report-dir` | Output directory for Excel and HTML reports |
| `--dry-run` | Report what each patch would do (modify / create / delete / rename) without changing any file |

### Reports

After a run, two files are created in `--report-dir`:

- `driftpatch-report-YYYYMMDD-HHMMSS.xlsx`
- `driftpatch-report-YYYYMMDD-HHMMSS.html`

Each row records patch path, target file, status (`success` / `skipped` / `failed`), error kind, and timestamps. A `skipped` row means the patch was already applied (idempotent detection) — it does not count as a failure.

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All patches applied successfully |
| `1` | One or more patches failed, or a fatal error occurred |

Failed patches are logged in the report; processing continues for remaining patches.

### Pre-apply conflict check

While `apply --dry-run` checks whether patches apply to a given `work_dir`, `check` inspects whether patches are mutually consistent without touching any working directory.

```bash
driftpatch-batch check --patch-dir C:\project\patch-repo\patches
```

Detected issues:

| Issue | Severity | Description |
|-------|----------|-------------|
| Overlapping hunk | error | Two hunks touch the same token range on the same file |
| Edit of deleted file | error | Modify / Rename-with-edit targeting a file that a Delete removes |
| Patch targets rename old path | warning | A patch whose `target_file` equals a Rename's `old_path` (order-dependent) |

Exit code: `1` if any error is found, `0` otherwise.

`apply` analyzes inter-patch dependencies before applying and automatically orders them by base priority `Create -> Modify -> Rename -> Delete`, plus Rename old/new path dependencies. This keeps a batch containing both a Modify on `Old.java` and a Rename `Old.java -> New.java` safe by applying the Modify first.

### Idempotent re-apply

Re-running `apply` against a work directory where some patches were already applied is safe: a `Modify` patch whose target already matches the patch's post-apply content (ignoring whitespace/indent differences) is detected as **already applied** and reported with status `skipped` instead of `failed` or `success`. No file write or `.bak` backup is created for a skipped patch. `Create` and pure `Rename` patches already had equivalent idempotent detection.

This detection is heuristic: for a hunk that only deletes tokens (empty `added_text`), "already applied" is detected by finding the hunk's context tokens adjacent to each other. If a hunk is only partially applied (e.g. one of two expected matches), it is still treated as drift and reported as `failed`, not skipped.

### Generate patches from a Git commit

```bash
driftpatch-batch from-commit \
  --repo C:\project \
  --commit abc1234 \
  --workdir C:\project \
  --patch-repo C:\project\patch-repo \
  --author alice \
  --description "REQ-123 fix null check" \
  --report-dir C:\project\reports
```

| Option | Description |
|--------|-------------|
| `--repo` | Git repository path |
| `--commit` | Commit SHA or ref |
| `--workdir` | Base directory for `target_file` paths |
| `--patch-repo` | Patch repository root (parent of `patches/`) |
| `--author` | Patch author (optional) |
| `--description` | Patch description (defaults to commit message) |
| `--report-dir` | Report output directory (optional) |

Multiple edits in the same file are split into separate `.dpatch` files per hunk.

All change types in the commit are captured: modified files become `modify` patches, added files become `create` patches, deleted files become `delete` patches (recording the deleted content's significant tokens as `verify_tokens`), and renames are detected and become `rename` patches. On apply, a `delete`/pure-`rename` patch only removes or moves the file if its current content still matches `verify_tokens` (whitespace/indent differences are ignored) — drifted files are left untouched and reported as failures.

## Patch Repository Layout

```
patch-repo/
└── patches/
    └── src/
        └── Foo.java/
            ├── 20260628-fix-null-check-a1b2c3d4.dpatch
            └── 20260629-add-logging-e5f6g7h8.dpatch
```

- Each patch is stored at `patches/<target_file>/<filename>.dpatch`.
- `target_file` is a path relative to `work_dir`, using `/` as the separator (e.g. `src/Foo.java`).
- Filenames follow `{YYYYMMDD}-{kebab-description}-{uuid8}.dpatch`.
- Legacy flat layout (`patches/*.dpatch` at the top level) is also supported for reading.

DriftPatch does **not** perform Git write operations (commit, push, etc.). It uses libgit2 (`git2` crate) read-only for importing patches from commit history. Version control is your responsibility.

## `.dpatch` File Format

A `.dpatch` file is JSON with the following structure.

### `PatchFile` (root)

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Format version (currently `"1"`) |
| `id` | string | Unique patch ID (`YYYYMMDD-kebab-uuid8`) |
| `author` | string | Author name from settings |
| `created_at` | string | Creation timestamp (ISO 8601) |
| `description` | string | Human-readable description |
| `target_file` | string | Path relative to `work_dir` (`/` separated) |
| `language` | string | Language profile name (e.g. `java`, `python`) |
| `encoding` | string | File encoding (e.g. `UTF-8`) |
| `hunks` | array | List of diff hunks |

### `DiffHunk`

| Field | Type | Description |
|-------|------|-------------|
| `context_before` | Token[] | Significant tokens immediately before the change |
| `removed` | Token[] | Tokens to remove |
| `added_text` | string | Replacement text (verbatim from edited source) |
| `context_after` | Token[] | Significant tokens immediately after the change |

### `Token`

| Field | Type | Description |
|-------|------|-------------|
| `kind` | string | `CODE`, `STRING_LITERAL`, `LINE_COMMENT`, `BLOCK_COMMENT`, `NEWLINE`, or `WHITESPACE` |
| `text` | string | Token text |

### Example

```json
{
  "version": "1",
  "id": "20260628-fix-null-check-a1b2c3d4",
  "author": "alice",
  "created_at": "2026-06-28T10:00:00+0900",
  "description": "fix null check",
  "target_file": "src/Foo.java",
  "language": "java",
  "encoding": "UTF-8",
  "hunks": [
    {
      "context_before": [],
      "removed": [],
      "added_text": "    Objects.requireNonNull(bar);\n",
      "context_after": []
    }
  ]
}
```

## Supported Language Profiles

| Profile | Extensions |
|---------|------------|
| Java | `.java` |
| Python | `.py` |
| C/C++ | `.c`, `.cpp`, `.cc`, `.cxx`, `.h`, `.hpp`, `.hxx`, `.rc` |
| SQL | `.sql` |
| JavaScript/TypeScript | `.js`, `.ts`, `.jsx`, `.tsx`, `.mjs`, `.cjs` |
| Rust | `.rs` |
| C# | `.cs`, `.csx` |
| Go | `.go` |
| PL/SQL | `.pls`, `.pks`, `.pkb`, `.pck`, `.psc`, `.plsql` |
| JSON | `.json` |
| YAML | `.yml`, `.yaml` |
| properties | `.properties` |
| XML/HTML | `.xml`, `.xsd`, `.xsl`, `.xslt`, `.svg`, `.xhtml`, `.html`, `.htm` |
| Generic | All other extensions |

Unrecognized extensions use the generic profile (line comments `//`, block comments `/* */`).

Notes on the config-file profiles: `properties` treats both `#` and `!` as line comments and disables quote-delimited strings entirely (a bare `'` in a value like `it's` would otherwise be misread as a string start); a value containing `#` or `!` is therefore treated as a comment from that point on, same as in a real `.properties` file. `YAML` treats `#` as a line comment outside of quoted strings, so `key: value  # note` works, but an unquoted `foo#bar` (no preceding space) is also read as a comment start, matching common YAML linters' expectations more loosely than a full YAML parser would.

## Custom Language Profiles

For languages not covered by the built-in profiles, place a `profiles.json` file next to `settings.json`:

- **Windows:** `%APPDATA%\DriftPatch\profiles.json`
- **Linux:** `~/.local/share/DriftPatch/profiles.json`
- **macOS:** `~/Library/Application Support/DriftPatch/profiles.json`

It is read once at startup by both `driftpatch` (GUI) and `driftpatch-batch` (CLI). If the file is absent, nothing happens. If it fails to parse, a warning is shown (GUI: initial status bar message; CLI: printed to stderr) and DriftPatch continues with only the built-in profiles — a broken `profiles.json` never prevents startup.

The file is a JSON array of profile definitions:

```json
[
  {
    "name": "hcl",
    "extensions": ["tf", "hcl"],
    "line_comments": ["#", "//"],
    "block_comment": ["/*", "*/"],
    "string_delimiters": ["\""],
    "triple_quote": false
  }
]
```

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Profile name, must be non-empty and unique among custom profiles |
| `extensions` | yes | File extensions this profile applies to (without the leading dot), must be non-empty |
| `line_comments` | no (default `[]`) | Line comment start markers; multiple are allowed (e.g. `["#", "!"]`) |
| `block_comment` | no (default none) | A `[start, end]` pair, e.g. `["/*", "*/"]` |
| `string_delimiters` | no (default `[]`) | Characters that start a string literal |
| `triple_quote` | no (default `false`) | Enable Python-style `'''`/`"""` triple-quoted strings |

Custom profiles take priority over built-in profiles for the same extension, so a custom profile can override a built-in one (e.g. redefining `.java` handling) if needed.


## Troubleshooting

| Problem | Cause / Solution |
|---------|------------------|
| **Patch repository path not set** | Open **Settings** and set the patch repository path |
| **work_dir not set** | Open **Settings** and set the work directory |
| **Open file is not under work_dir** | The file must be inside the configured work directory |
| **No changes found** | Original and edited text are identical |
| **Hunk not unique (generation)** | The change pattern matches multiple locations; include more surrounding context in your edit |
| **Hunk not found (apply)** | Target source has drifted; regenerate or adjust the patch |
| **Ambiguous match (apply)** | Multiple locations match; manual review required |
| **Target file not found (batch)** | Check `work_dir` and `target_file` in the patch |
| **File open error** | Verify the file exists and is readable |


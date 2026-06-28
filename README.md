# DriftPatch

**DriftPatch** is a desktop tool for creating, managing, and applying source-code patches. It generates token-aware `.dpatch` files (JSON) from before/after text, stores them in a patch repository, and applies them to target files — either interactively via a GUI or in bulk via a CLI.

> Japanese documentation: [README.jp.md](README.jp.md)

## Features

- **GUI editor** — Three-column layout: original (read-only), editable working copy, and patch preview
- **Token-aware patching** — Language profiles for Java, Python, C/C++, SQL, JavaScript/TypeScript, and a generic fallback
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

### Three-column layout

| Column | Label | Purpose |
|--------|-------|---------|
| Left | Original (read-only) | Baseline text before changes |
| Center | Editable | Working copy for creating patches |
| Right | Patch preview (read-only) | Result of applying the selected patch |

The left and right columns scroll in sync with the center column.

### Patch list panel

The bottom panel lists patches whose `target_file` matches the currently open file. Use **Refresh** to reload from disk.

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

### Reports

After a run, two files are created in `--report-dir`:

- `driftpatch-report-YYYYMMDD-HHMMSS.xlsx`
- `driftpatch-report-YYYYMMDD-HHMMSS.html`

Each row records patch path, target file, status (`success` / `failed`), error kind, and timestamps.

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | All patches applied successfully |
| `1` | One or more patches failed, or a fatal error occurred |

Failed patches are logged in the report; processing continues for remaining patches.

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
| Generic | All other extensions |

Unrecognized extensions use the generic profile (line comments `//`, block comments `/* */`).

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


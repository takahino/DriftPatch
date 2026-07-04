//! English catalog. Keys must stay in sync with `ja.rs`
//! (enforced by the catalog consistency tests in `mod.rs`).

pub static CATALOG: &[(&str, &str)] = &[
    // --- Patch application (applier) ---
    ("apply.no_match", "Hunk {hunk}: no matching location found"),
    (
        "apply.count_mismatch",
        "Hunk {hunk}: expected {expected} match(es) but found {actual}. Positions: {positions}",
    ),
    (
        "apply.overlapping",
        "Hunk {hunk}: replacement ranges of multiple matches overlap",
    ),
    // --- Patch generation (generator) ---
    ("gen.no_diff", "No changes found"),
    // --- Content verification (verify) ---
    (
        "verify.mismatch",
        "expected {expected} token(s) / actual {actual}, first difference at: {index}",
    ),
    // --- Patch kind labels ---
    ("kind.modify", "Modify"),
    ("kind.create", "Create"),
    ("kind.delete", "Delete"),
    ("kind.rename", "Rename"),
    // --- Patch validation (model) ---
    (
        "model.no_old_path_for_kind",
        "old_path cannot be specified for a {kind} patch",
    ),
    (
        "model.delete_requires_verify",
        "A delete patch requires verify_tokens (content verification data)",
    ),
    ("model.delete_no_hunks", "A delete patch cannot have hunks"),
    (
        "model.rename_requires_old_path",
        "A rename patch requires old_path",
    ),
    (
        "model.pure_rename_requires_verify",
        "A rename patch without content changes requires verify_tokens",
    ),
    // --- Planned actions ---
    ("action.modify", "Applied"),
    ("action.create", "File created"),
    ("action.delete", "File deleted"),
    ("action.rename", "Renamed: {from} → {to}"),
    ("action.already_applied", "Already applied (no changes)"),
    // --- File operations (file_ops) ---
    ("fops.read_error", "File read error: {path}: {err}"),
    ("fops.write_error", "File write error: {path}: {err}"),
    ("fops.mkdir_error", "Directory creation error: {path}: {err}"),
    ("fops.delete_error", "File deletion error: {path}: {err}"),
    ("fops.rename_error", "Rename error: {from} → {to}: {err}"),
    ("fops.backup_error", "Backup creation failed: {path}: {err}"),
    (
        "fops.target_deleted_earlier",
        "Target file was already deleted by an earlier patch: {path}",
    ),
    ("fops.target_not_found", "Target file not found: {path}"),
    (
        "fops.already_exists",
        "A file with different content already exists at the destination: {path}",
    ),
    (
        "fops.delete_verification_failed",
        "Deletion aborted. File content does not match the patch record (drift detected): {path} ({mismatch})",
    ),
    (
        "fops.rename_verification_failed",
        "Rename aborted. Source file content does not match the patch record (drift detected): {path} ({mismatch})",
    ),
    ("common.invalid_patch", "Invalid patch: {msg}"),
    (
        "fops.delete_missing_verify",
        "Delete patch has no verify_tokens",
    ),
    (
        "fops.rename_missing_old_path",
        "Rename patch has no old_path",
    ),
    (
        "fops.rename_missing_verify",
        "Rename patch has no verify_tokens",
    ),
    // --- Patch repository ---
    ("repo.io", "I/O error: {err}"),
    ("repo.json", "JSON error: {err}"),
    ("repo.invalid_path", "Invalid path: {path}"),
    (
        "repo.unsupported_version",
        "Unsupported patch format version: {version} (it may have been created by a newer DriftPatch)",
    ),
    ("common.empty_target", "target_file is empty"),
    // --- Batch apply ---
    ("batch.list_error", "Patch enumeration error: {err}"),
    (
        "batch.report_dir_error",
        "Report directory creation error: {err}",
    ),
    ("batch.xlsx_error", "Excel report output error: {err}"),
    ("batch.html_error", "HTML report output error: {err}"),
    // --- Conflict check ---
    (
        "check.overlapping_hunk",
        "Overlapping hunks: hunk {hunk_a} of {patch_a} and hunk {hunk_b} of {patch_b} touch overlapping ranges in the same file {target}",
    ),
    (
        "check.modify_deleted",
        "Edit of deleted file: {edit_patch} edits {target}, which is deleted by {delete_patch}",
    ),
    (
        "check.rename_old_path",
        "Patch targets rename old path: {patch} targets {old_path}, the old path of {rename_patch} (order-dependent)",
    ),
    // --- Reports ---
    ("report.title", "DriftPatch Apply Report"),
    ("report.dryrun_xlsx", "DRY-RUN (no files were modified)"),
    (
        "report.dryrun_html",
        "DRY-RUN: only applicability was checked. No files were modified.",
    ),
    ("report.summary", "Summary"),
    ("report.h.patch_path", "Patch path"),
    ("report.h.patch_id", "Patch ID"),
    ("report.h.target", "Target file"),
    ("report.h.status", "Status"),
    ("report.h.action", "Action"),
    ("report.h.error_kind", "Error kind"),
    ("report.h.hunk", "Hunk index"),
    ("report.h.message", "Message"),
    ("report.h.started", "Started at"),
    ("report.h.finished", "Finished at"),
    ("report.h.start_short", "Start"),
    ("report.h.end_short", "End"),
    ("report.total", "Total"),
    ("report.success", "Success"),
    ("report.skipped", "Skipped"),
    ("report.failed", "Failed"),
    // --- Git commit import ---
    ("fc.saved", "Generated and saved"),
    ("git.not_a_repo", "Not a Git repository"),
    ("git.invalid_commit", "Invalid commit: {commit}"),
    ("git.git2", "Git error: {err}"),
    ("git.io", "I/O error: {err}"),
    ("git.skip_binary", "Binary file skipped"),
    ("git.no_rename_old", "Cannot resolve the rename source path"),
    (
        "git.no_after_content",
        "Cannot read the file content after the commit",
    ),
    (
        "git.no_parent_for_delete",
        "Cannot read the pre-deletion content because the commit has no parent",
    ),
    (
        "git.no_before_content",
        "Cannot read the file content before deletion",
    ),
    (
        "git.no_parent_for_rename",
        "Cannot read the rename source content because the commit has no parent",
    ),
    (
        "git.no_rename_before",
        "Cannot read the file content before the rename",
    ),
    (
        "git.no_rename_after",
        "Cannot read the file content after the rename",
    ),
    ("git.empty_path", "Path is empty"),
    (
        "git.path_outside",
        "Invalid path (outside work_dir): {path}",
    ),
    (
        "git.not_under_workdir_missing",
        "File not found under work_dir: {path} (work_dir: {work_dir})",
    ),
    (
        "git.not_under_workdir",
        "Target file is not under work_dir: {path}",
    ),
    // --- CLI (driftpatch-batch) ---
    ("cli.about", "DriftPatch batch patch-apply CLI"),
    (
        "cli.lang_help",
        "Message language (ja / en). Can also be set via the DRIFTPATCH_LANG environment variable",
    ),
    (
        "cli.apply.about",
        "Apply all patches using the given workdir and patch dir",
    ),
    ("cli.apply.workdir", "Working directory containing target files"),
    (
        "cli.apply.patch_dir",
        "Directory containing .dpatch files (patches/ or the repo root)",
    ),
    ("cli.apply.report_dir", "Output directory for reports"),
    (
        "cli.apply.dry_run",
        "Report what each patch would do without changing any file",
    ),
    ("cli.fc.about", "Generate .dpatch files from a Git commit"),
    ("cli.fc.repo", "Git repository path"),
    ("cli.fc.commit", "Commit SHA or ref"),
    ("cli.fc.workdir", "Base directory for target_file paths"),
    (
        "cli.fc.patch_repo",
        "Patch repository root (parent of patches/)",
    ),
    ("cli.fc.author", "Patch author"),
    (
        "cli.fc.description",
        "Patch description (defaults to the commit message)",
    ),
    ("cli.fc.report_dir", "Report output directory (optional)"),
    (
        "cli.check.about",
        "Check whether patches in patch-dir are mutually consistent without applying them",
    ),
    (
        "cli.check.patch_dir",
        "Directory containing .dpatch files (patches/ or the repo root)",
    ),
    (
        "cli.apply.dry_run_done",
        "dry-run finished (no files were modified)",
    ),
    ("cli.apply.done", "Batch apply finished"),
    (
        "cli.summary_line",
        "  Total: {total} / Success: {success} / Skipped: {skipped} / Failed: {failed}",
    ),
    ("cli.error", "Error: {err}"),
    ("cli.fc.done", "Patch generation from Git commit finished"),
    (
        "cli.fc.summary",
        "  Saved: {saved} / Skipped: {skipped} / Failed: {failed}",
    ),
    ("cli.check.ok", "OK: no conflicts detected ({dir})"),
    ("cli.check.warnings", "Warnings ({count}):"),
    ("cli.check.errors", "Conflicts ({count}):"),
    // --- GUI ---
    ("gui.open_prompt", "Open a file to get started"),
    (
        "gui.opened_file",
        "Opened: {file} | Language: {lang} | Encoding: {enc}",
    ),
    ("gui.open_error", "File open error: {err}"),
    ("gui.patch_list_error", "Failed to load patch list: {err}"),
    (
        "gui.workdir_not_set_check",
        "work_dir is not set (check the settings)",
    ),
    ("gui.workdir_not_set", "work_dir is not set"),
    ("gui.patch_no_target", "Patch has no target_file"),
    ("gui.no_file_open", "No file is open"),
    (
        "gui.repo_not_set_check",
        "Patch repository path is not set (check the settings)",
    ),
    ("gui.repo_not_set", "Patch repository path is not set"),
    ("gui.patch_saved", "Patch saved: {path}"),
    ("gui.patch_save_error", "Patch save error: {err}"),
    (
        "gui.patch_not_for_open_file",
        "The selected patch does not target the currently open file",
    ),
    (
        "gui.delete_preview_ok",
        "Delete patch: applying will delete this file (content verified OK)",
    ),
    (
        "gui.delete_preview_drift",
        "Delete patch: content does not match the patch record (drift detected): {mismatch}",
    ),
    (
        "gui.delete_preview_invalid",
        "Delete patch: verify_tokens is missing (invalid patch)",
    ),
    ("gui.create_preview", "Create patch: applying will create {path}"),
    ("gui.preview_failed", "Preview failed: {err}"),
    ("gui.rename_preview", "Rename patch: {from} → {to}"),
    ("gui.preview_updated", "Preview updated"),
    (
        "gui.apply_fail_no_match",
        "Apply failed: no matching location for hunk {hunk}",
    ),
    (
        "gui.apply_fail_count",
        "Apply failed: hunk {hunk} expected {expected} match(es) but found {actual} (drift detected).",
    ),
    (
        "gui.apply_fail_overlap",
        "Apply failed: replacement ranges of multiple matches overlap for hunk {hunk}.",
    ),
    (
        "gui.rename_needs_workdir",
        "Applying a rename patch requires work_dir to be set",
    ),
    ("gui.rename_applied", "Rename applied: {from} → {to}"),
    ("gui.rename_patch_status", "Rename patch: {desc}"),
    ("gui.apply_error", "Patch apply error: {err}"),
    (
        "gui.applied_with_backup",
        "Patch applied: saved to {path}, backup: {bak}",
    ),
    ("gui.applied", "Patch applied: saved to {path}"),
    (
        "gui.delete_applied_with_backup",
        "Delete patch applied: deleted {path}, backup: {bak}",
    ),
    ("gui.delete_applied", "Delete patch applied: deleted {path}"),
    ("gui.apply_status", "Patch applied: {desc}"),
    (
        "gui.git_repo_not_set",
        "Git repository path or work_dir is not set",
    ),
    ("gui.git_history_error", "Failed to load Git history: {err}"),
    (
        "gui.git_import_done",
        "Git import finished: {saved} saved, {skipped} skipped",
    ),
    (
        "gui.git_import_partial",
        "Git import: {saved} saved, {skipped} skipped, {failed} failed to save",
    ),
    ("gui.git_import_skipped_header", "Skipped:"),
    ("gui.git_import_more", "... and {count} more"),
    ("gui.git_import_save_errors", "Save errors:"),
    ("gui.git_import_error", "Git import error: {err}"),
    ("gui.patch_deleted", "Deleted: {path}"),
    ("gui.patch_delete_error", "Delete error: {err}"),
    ("gui.btn_open", "📂 Open file"),
    ("gui.dlg_open_title", "Open file"),
    ("gui.btn_git_import", "📜 Import from Git commit"),
    ("gui.btn_generate", "🔧 Generate patch..."),
    ("gui.btn_settings", "⚙ Settings"),
    ("gui.status_file", "📄 {file}  |  Lang: {lang}  |  ENC: {enc}"),
    ("gui.win_generate", "Generate patch"),
    (
        "gui.generate_desc_label",
        "Patch description (e.g. requirement ID):",
    ),
    ("gui.btn_do_generate", "Generate"),
    ("gui.btn_cancel", "Cancel"),
    ("gui.panel_patches", "Patches"),
    ("gui.btn_refresh", "🔄 Refresh"),
    ("gui.btn_apply", "▶ Apply"),
    ("gui.btn_delete", "🗑 Delete"),
    (
        "gui.warn_repo_not_set",
        "⚠ Patch repository path is not set (use the Settings button)",
    ),
    ("gui.hint_open_file", "Open a file to see its patches here"),
    (
        "gui.warn_workdir_not_set",
        "⚠ work_dir is not set (use the Settings button)",
    ),
    (
        "gui.warn_not_under_workdir",
        "⚠ The open file is not under work_dir",
    ),
    ("gui.no_patches_for_file", "No patches for this file: {path}"),
    ("gui.target_label", "Target: {path}"),
    ("gui.col_patch", "Patch"),
    ("gui.col_kind", "Kind"),
    ("gui.col_author", "Author"),
    ("gui.col_desc", "Description"),
    ("gui.col_created", "Created at"),
    ("gui.win_settings", "Settings"),
    ("gui.set_username", "Username:"),
    ("gui.set_repo_path", "Patch repository path:"),
    ("gui.pick_repo", "Select patch repository folder"),
    ("gui.set_git_path", "Git repository path:"),
    ("gui.pick_git", "Select Git repository folder"),
    ("gui.set_workdir", "Work directory:"),
    ("gui.pick_workdir", "Select work directory"),
    ("gui.set_backup", "Create .bak on apply:"),
    ("gui.enabled", "Enabled"),
    ("gui.btn_save_close", "Save and close"),
    ("gui.set_language", "言語 / Language:"),
    ("gui.set_font_size", "Font size:"),
    ("gui.win_git_import", "Generate patches from Git commit"),
    (
        "gui.git_select_hint",
        "Select a commit or enter a SHA / ref directly.",
    ),
    ("gui.git_sha_label", "Commit SHA / ref:"),
    (
        "gui.git_desc_label",
        "Description (defaults to the commit message):",
    ),
    ("gui.git_recent", "Recent commits:"),
    ("gui.git_sha_required", "Please specify a commit SHA"),
    ("gui.col_original", "Original (read-only)"),
    ("gui.col_editable", "Editable"),
    ("gui.col_preview", "Patch preview"),
    ("gui.btn_batch", "📦 Batch apply"),
    ("gui.win_batch", "Batch apply / Conflict check"),
    ("gui.batch_patch_dir", "Patch directory:"),
    ("gui.batch_report_dir", "Report directory:"),
    ("gui.batch_dry_run", "Dry run (no changes):"),
    ("gui.batch_btn_dry_run", "▶ Run dry-run"),
    ("gui.batch_btn_apply", "▶ Apply"),
    ("gui.batch_btn_check", "🔍 Check conflicts"),
    (
        "gui.batch_apply_warning",
        "⚠ Dry run is disabled. Running this will modify files.",
    ),
    ("gui.batch_apply_result", "Apply result"),
    (
        "gui.batch_apply_summary",
        "Total: {total} / Success: {success} / Skipped: {skipped} / Failed: {failed}",
    ),
    ("gui.batch_check_result", "Conflict check result"),
    ("gui.batch_check_ok", "OK: no conflicts detected"),
    // --- Custom language profiles ---
    ("custom_profile.empty_name", "Profile name is empty"),
    (
        "custom_profile.empty_extensions",
        "Profile '{name}' has no extensions",
    ),
    (
        "custom_profile.duplicate_name",
        "Duplicate profile name: {name}",
    ),
    (
        "custom_profile.read_error",
        "Failed to read profiles.json: {path}: {err}",
    ),
    (
        "custom_profile.parse_error",
        "Failed to parse profiles.json: {path}: {err}",
    ),
];

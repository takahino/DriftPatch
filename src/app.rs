use std::path::{Path, PathBuf};

use driftpatch::encoding::read_file_auto;
use driftpatch::git_import::{generate_patches_from_commit, list_commits, CommitInfo};
use driftpatch::i18n::{self, tr, tr_args};
use driftpatch::lexer::profiles::detect_profile;
use driftpatch::patch::context::ContextConfig;
use driftpatch::patch::file_ops::backup_path;
use driftpatch::patch::model::{PatchFile, PatchKind};
use driftpatch::patch::name_gen::generate_filename;
use driftpatch::patch::repository::PatchRepository;
use driftpatch::patch::verify::verify_significant_tokens;
use driftpatch::patch::{
    apply_patch, generate_patch, ApplyError, ApplyOptions, PatchWorkspace, PlannedAction,
};

/// 永続化する設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// patches/ リポジトリのベースパス
    pub patch_repo_path: String,
    /// パッチ生成時の作者名
    pub username: String,
    /// パッチ相対パスの基準ディレクトリ
    pub work_dir: String,
    /// Git リポジトリパス（空の場合は work_dir を使用）
    #[serde(default)]
    pub git_repo_path: String,
    /// パッチ適用時に .bak バックアップを作成するか（GUI のみ）
    #[serde(default = "default_create_backup")]
    pub create_backup: bool,
    /// UI 言語（"ja" / "en"）
    #[serde(default = "default_ui_language")]
    pub ui_language: String,
}

fn default_create_backup() -> bool {
    true
}

fn default_ui_language() -> String {
    "ja".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            patch_repo_path: String::new(),
            username: String::new(),
            work_dir: String::new(),
            git_repo_path: String::new(),
            create_backup: default_create_backup(),
            ui_language: default_ui_language(),
        }
    }
}

impl Settings {
    fn settings_path() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("DriftPatch").join("settings.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::settings_path() {
            if let Ok(data) = std::fs::read(&path) {
                if let Ok(s) = serde_json::from_slice(&data) {
                    return s;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::settings_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(path, json.as_bytes());
            }
        }
    }
}

/// Git コミットからパッチ取り込みダイアログの状態
#[derive(Debug, Default)]
pub struct GitImportDialog {
    pub commits: Vec<CommitInfo>,
    pub selected: Option<usize>,
    pub commit_input: String,
    pub description: String,
    pub result_message: Option<String>,
    pub error: Option<String>,
    pub loading: bool,
}

/// パッチ生成ダイアログの状態
#[derive(Debug, Default)]
pub struct GeneratePatchDialog {
    pub description: String,
    pub error: Option<String>,
    pub warning: Option<String>,
}

/// アプリケーション全体の状態
pub struct DriftPatchApp {
    /// 左列: 元テキスト（読取専用）
    pub original_text: String,
    /// 中列: 編集中テキスト
    pub edited_text: String,
    /// 右列: パッチ適用後プレビュー
    pub preview_text: String,
    /// 3列共有スクロールオフセット（ピクセル）
    pub scroll_offset: f32,
    /// 開いているファイルのパス
    pub file_path: Option<PathBuf>,
    /// 現在の言語プロファイル名
    pub language: &'static str,
    /// 現在の文字コード
    pub encoding: String,
    /// パッチ一覧（patches/ からの相対パス, パッチ本体）
    pub patches: Vec<(String, PatchFile)>,
    /// 選択中のパッチ（patches/ からの相対パス）
    pub selected_patch: Option<String>,
    /// 設定
    pub settings: Settings,
    /// ステータスバーメッセージ
    pub status_message: String,
    /// 設定ウィンドウ表示フラグ
    pub show_settings: bool,
    /// パッチ生成ダイアログ
    pub generate_patch_dialog: Option<GeneratePatchDialog>,
    /// Git コミット取り込みダイアログ
    pub git_import_dialog: Option<GitImportDialog>,
}

impl Default for DriftPatchApp {
    fn default() -> Self {
        let settings = Settings::load();
        // 初回フレーム描画前に UI 言語を確定させる
        if let Some(lang) = i18n::lang_from_str(&settings.ui_language) {
            i18n::set_lang(lang);
        }
        Self {
            original_text: String::new(),
            edited_text: String::new(),
            preview_text: String::new(),
            scroll_offset: 0.0,
            file_path: None,
            language: "generic",
            encoding: "UTF-8".to_string(),
            patches: Vec::new(),
            selected_patch: None,
            settings,
            status_message: tr("gui.open_prompt").to_string(),
            show_settings: false,
            generate_patch_dialog: None,
            git_import_dialog: None,
        }
    }
}

impl DriftPatchApp {
    /// ファイルを開いてエディタに読み込む
    pub fn open_file(&mut self, path: PathBuf) {
        match read_file_auto(&path) {
            Ok((text, enc)) => {
                let profile = detect_profile(&path);
                self.language = profile.name;
                self.encoding = enc.clone();
                self.original_text = text.clone();
                self.edited_text = text;
                self.preview_text = String::new();
                self.file_path = Some(path.clone());
                self.selected_patch = None;

                self.reload_patches();

                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                self.status_message = tr_args(
                    "gui.opened_file",
                    &[("file", filename), ("lang", self.language), ("enc", &enc)],
                );
            }
            Err(e) => {
                self.status_message = tr_args("gui.open_error", &[("err", &e.to_string())]);
            }
        }
    }

    /// パッチ一覧をリポジトリから再読み込みする
    pub fn reload_patches(&mut self) {
        if self.settings.patch_repo_path.is_empty() {
            return;
        }
        let repo = PatchRepository::new(&self.settings.patch_repo_path);
        match repo.list() {
            Ok(list) => {
                self.patches = list;
                if let Some(ref selected) = self.selected_patch {
                    if !self.patches.iter().any(|(p, _)| p == selected) {
                        self.selected_patch = None;
                        self.preview_text = String::new();
                    }
                }
            }
            Err(e) => {
                self.status_message = tr_args("gui.patch_list_error", &[("err", &e.to_string())]);
            }
        }
    }

    /// 開いているファイル向けのパッチだけを返す
    pub fn patches_for_open_file(&self) -> Vec<(String, PatchFile)> {
        let Some(ref rel) = self.open_file_relative() else {
            return Vec::new();
        };
        self.patches
            .iter()
            .filter(|(_, patch)| {
                patch.target_file == *rel
                    // リネームパッチは移動元ファイルを開いている場合にも表示する
                    || (patch.kind == PatchKind::Rename
                        && patch.old_path.as_deref() == Some(rel.as_str()))
            })
            .cloned()
            .collect()
    }

    /// 開いているファイルの work_dir 相対パス（`/` 区切り）
    pub fn open_file_relative(&self) -> Option<String> {
        let file_path = self.file_path.as_ref()?;
        self.target_file_relative(file_path).ok()
    }

    /// パッチ相対パスからパッチ本体を取得する
    fn patch_by_path(&self, patch_path: &str) -> Option<&PatchFile> {
        self.patches
            .iter()
            .find(|(p, _)| p == patch_path)
            .map(|(_, patch)| patch)
    }

    /// work_dir 基準の相対パスを target_file 用文字列に変換する
    fn target_file_relative(&self, file_path: &std::path::Path) -> Result<String, String> {
        let work_dir = self.settings.work_dir.trim();
        if work_dir.is_empty() {
            return Err(tr("gui.workdir_not_set_check").to_string());
        }
        let work_path = std::path::Path::new(work_dir);
        let rel = file_path.strip_prefix(work_path).map_err(|_| {
            tr_args(
                "git.not_under_workdir",
                &[("path", &file_path.display().to_string())],
            )
        })?;
        Ok(rel.to_str().unwrap_or("").replace('\\', "/"))
    }

    /// パッチの target_file から絶対パスを解決する
    fn resolve_target_file(&self, patch: &PatchFile) -> Result<PathBuf, String> {
        if self.settings.work_dir.trim().is_empty() {
            return Err(tr("gui.workdir_not_set").to_string());
        }
        if patch.target_file.is_empty() {
            return Err(tr("gui.patch_no_target").to_string());
        }
        Ok(std::path::Path::new(&self.settings.work_dir).join(&patch.target_file))
    }

    /// 元テキストと編集テキストからパッチを生成して保存する
    pub fn generate_and_save_patch(&mut self, description: &str) -> Result<(), String> {
        let Some(ref file_path) = self.file_path.clone() else {
            return Err(tr("gui.no_file_open").to_string());
        };

        if self.settings.patch_repo_path.is_empty() {
            return Err(tr("gui.repo_not_set_check").to_string());
        }

        let target_file = self.target_file_relative(file_path)?;
        let profile = detect_profile(file_path);
        let config = ContextConfig::default();

        match generate_patch(
            &self.original_text,
            &self.edited_text,
            profile,
            &self.settings.username,
            description,
            &target_file,
            &self.encoding,
            &config,
        ) {
            Ok(patch) => {
                let filename = generate_filename(description);
                let repo = PatchRepository::new(&self.settings.patch_repo_path);
                match repo.save(&patch, &filename) {
                    Ok(saved_path) => {
                        self.status_message = tr_args(
                            "gui.patch_saved",
                            &[("path", &saved_path.display().to_string())],
                        );
                        self.reload_patches();
                        Ok(())
                    }
                    Err(e) => Err(tr_args("gui.patch_save_error", &[("err", &e.to_string())])),
                }
            }
            // GeneratorError の Display は i18n 済み
            Err(e) => Err(e.to_string()),
        }
    }

    /// 選択中のパッチを元テキストに適用してプレビューを更新する
    pub fn update_preview(&mut self) {
        let Some(ref patch_path) = self.selected_patch.clone() else {
            self.preview_text = String::new();
            return;
        };
        let Some(patch) = self.patch_by_path(patch_path).cloned() else {
            self.preview_text = String::new();
            return;
        };

        let Some(ref file_path) = self.file_path.clone() else {
            return;
        };

        // リネームパッチは移動元ファイルを開いている場合に有効（target_file は新パス）
        let applies_to_open_file = if patch.kind == PatchKind::Rename {
            let rel = self.open_file_relative();
            rel.as_deref() == patch.old_path.as_deref()
        } else {
            self.resolve_target_file(&patch).ok().as_ref() == Some(file_path)
        };
        if !applies_to_open_file {
            self.preview_text = String::new();
            self.status_message = tr("gui.patch_not_for_open_file").to_string();
            return;
        }

        let profile = detect_profile(file_path);

        match patch.kind {
            PatchKind::Delete => {
                // 削除パッチにプレビューはない。内容検証の結果だけ伝える
                self.preview_text = String::new();
                self.status_message = match patch.verify_tokens.as_deref() {
                    Some(expected) => {
                        match verify_significant_tokens(&self.original_text, profile, expected) {
                            Ok(()) => tr("gui.delete_preview_ok").to_string(),
                            Err(m) => {
                                tr_args("gui.delete_preview_drift", &[("mismatch", &m.to_string())])
                            }
                        }
                    }
                    None => tr("gui.delete_preview_invalid").to_string(),
                };
                return;
            }
            PatchKind::Create => {
                // Create は空文字列への適用で作成される全文をプレビューする
                match apply_patch("", &patch, profile) {
                    Ok(text) => {
                        self.preview_text = text;
                        self.status_message =
                            tr_args("gui.create_preview", &[("path", &patch.target_file)]);
                    }
                    Err(e) => {
                        self.preview_text = String::new();
                        self.status_message =
                            tr_args("gui.preview_failed", &[("err", &e.to_string())]);
                    }
                }
                return;
            }
            PatchKind::Rename => {
                // 移動後の内容（純リネームなら現内容そのまま）をプレビューする
                let preview = if patch.hunks.is_empty() {
                    Ok(self.original_text.clone())
                } else {
                    apply_patch(&self.original_text, &patch, profile)
                };
                match preview {
                    Ok(text) => {
                        self.preview_text = text;
                        self.status_message = tr_args(
                            "gui.rename_preview",
                            &[
                                ("from", patch.old_path.as_deref().unwrap_or("?")),
                                ("to", &patch.target_file),
                            ],
                        );
                    }
                    Err(e) => {
                        self.preview_text = String::new();
                        self.status_message =
                            tr_args("gui.preview_failed", &[("err", &e.to_string())]);
                    }
                }
                return;
            }
            PatchKind::Modify => {}
        }

        match apply_patch(&self.original_text, &patch, profile) {
            Ok(result) => {
                self.preview_text = result;
                self.status_message = tr("gui.preview_updated").to_string();
            }
            Err(ApplyError::NoMatch { hunk_index }) => {
                self.preview_text = String::new();
                self.status_message = tr_args(
                    "gui.apply_fail_no_match",
                    &[("hunk", &hunk_index.to_string())],
                );
            }
            Err(ApplyError::CountMismatch {
                hunk_index,
                expected,
                actual,
                ..
            }) => {
                self.preview_text = String::new();
                self.status_message = tr_args(
                    "gui.apply_fail_count",
                    &[
                        ("hunk", &hunk_index.to_string()),
                        ("expected", &expected.to_string()),
                        ("actual", &actual.to_string()),
                    ],
                );
            }
            Err(ApplyError::OverlappingMatches { hunk_index }) => {
                self.preview_text = String::new();
                self.status_message = tr_args(
                    "gui.apply_fail_overlap",
                    &[("hunk", &hunk_index.to_string())],
                );
            }
        }
    }

    /// 選択中のパッチをファイルに適用し、エディタ状態を更新する。
    /// kind に応じて変更・作成・削除・リネームのファイル操作を行う（.bak 作成含む）。
    pub fn apply_selected_patch(&mut self) {
        let Some(ref patch_path) = self.selected_patch.clone() else {
            return;
        };
        let Some(patch) = self.patch_by_path(patch_path).cloned() else {
            return;
        };

        let Some(ref file_path) = self.file_path.clone() else {
            return;
        };

        let opts = ApplyOptions {
            dry_run: false,
            create_backup: self.settings.create_backup,
        };

        if patch.kind == PatchKind::Rename {
            // リネームは旧・新の 2 パスが必要なため work_dir 基準で適用する
            let work_dir = self.settings.work_dir.trim().to_string();
            if work_dir.is_empty() {
                self.status_message = tr("gui.rename_needs_workdir").to_string();
                return;
            }
            let mut ws = PatchWorkspace::new(&work_dir);
            match ws.apply(&patch, &opts) {
                Ok(PlannedAction::Rename { from, to }) => {
                    // 移動先のファイルを開き直してエディタ状態を追従させる
                    let new_abs = std::path::Path::new(&work_dir)
                        .join(to.replace('/', std::path::MAIN_SEPARATOR_STR));
                    self.open_file(new_abs);
                    self.status_message =
                        tr_args("gui.rename_applied", &[("from", &from), ("to", &to)]);
                }
                Ok(action) => {
                    self.status_message = tr_args(
                        "gui.rename_patch_status",
                        &[("desc", &action.describe(false))],
                    );
                }
                Err(e) => {
                    self.status_message = tr_args("gui.apply_error", &[("err", &e.to_string())]);
                }
            }
            return;
        }

        // Modify / Create / Delete は開いているファイルに対して直接適用する
        let mut ws = PatchWorkspace::new(
            file_path
                .parent()
                .unwrap_or_else(|| std::path::Path::new(".")),
        );
        match ws.apply_at(Some(file_path), &patch, &opts) {
            Ok(PlannedAction::Modify) => {
                let result = ws.cached_text_at(file_path).unwrap_or_default().to_string();
                self.original_text = result.clone();
                self.edited_text = result;
                self.preview_text = String::new();
                self.status_message = if self.settings.create_backup {
                    tr_args(
                        "gui.applied_with_backup",
                        &[
                            ("path", &file_path.display().to_string()),
                            ("bak", &backup_path(file_path).display().to_string()),
                        ],
                    )
                } else {
                    tr_args("gui.applied", &[("path", &file_path.display().to_string())])
                };
            }
            Ok(PlannedAction::Delete) => {
                // 対象ファイルが消えたためエディタを閉じる
                self.file_path = None;
                self.original_text = String::new();
                self.edited_text = String::new();
                self.preview_text = String::new();
                self.status_message = if self.settings.create_backup {
                    tr_args(
                        "gui.delete_applied_with_backup",
                        &[
                            ("path", &file_path.display().to_string()),
                            ("bak", &backup_path(file_path).display().to_string()),
                        ],
                    )
                } else {
                    tr_args(
                        "gui.delete_applied",
                        &[("path", &file_path.display().to_string())],
                    )
                };
            }
            Ok(action) => {
                self.status_message =
                    tr_args("gui.apply_status", &[("desc", &action.describe(false))]);
            }
            Err(e) => {
                self.status_message = tr_args("gui.apply_error", &[("err", &e.to_string())]);
            }
        }
    }

    /// Git リポジトリパスを解決する（未設定時は work_dir）
    pub fn resolve_git_repo_path(&self) -> Option<PathBuf> {
        let path = self.settings.git_repo_path.trim();
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
        let work = self.settings.work_dir.trim();
        if work.is_empty() {
            None
        } else {
            Some(PathBuf::from(work))
        }
    }

    /// Git コミット取り込みダイアログを開く
    pub fn open_git_import(&mut self) {
        let Some(repo_path) = self.resolve_git_repo_path() else {
            self.status_message = tr("gui.git_repo_not_set").to_string();
            return;
        };

        match list_commits(&repo_path, 100) {
            Ok(commits) => {
                self.git_import_dialog = Some(GitImportDialog {
                    commits,
                    ..Default::default()
                });
            }
            Err(e) => {
                self.status_message = tr_args("gui.git_history_error", &[("err", &e.to_string())]);
            }
        }
    }

    /// 指定コミットからパッチを生成してリポジトリに保存する
    pub fn import_from_commit(&mut self, commit_sha: &str, description: &str) {
        let Some(repo_path) = self.resolve_git_repo_path() else {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error = Some(tr("gui.git_repo_not_set").to_string());
            }
            return;
        };

        if self.settings.patch_repo_path.is_empty() {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error = Some(tr("gui.repo_not_set").to_string());
            }
            return;
        }

        let work_dir = self.settings.work_dir.trim();
        if work_dir.is_empty() {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error = Some(tr("gui.workdir_not_set").to_string());
            }
            return;
        }

        let config = ContextConfig::default();
        let description_override = if description.trim().is_empty() {
            None
        } else {
            Some(description.trim())
        };

        match generate_patches_from_commit(
            &repo_path,
            commit_sha,
            Path::new(work_dir),
            &self.settings.username,
            description_override,
            &config,
        ) {
            Ok(result) => {
                let repo = PatchRepository::new(&self.settings.patch_repo_path);
                let mut saved = 0usize;
                let mut save_errors = Vec::new();

                for item in &result.generated {
                    match repo.save(&item.patch, &item.filename) {
                        Ok(_) => saved += 1,
                        Err(e) => save_errors.push(format!("{}: {}", item.target_file, e)),
                    }
                }

                self.reload_patches();

                let skipped_count = result.skipped.len();
                let msg = if save_errors.is_empty() {
                    tr_args(
                        "gui.git_import_done",
                        &[
                            ("saved", &saved.to_string()),
                            ("skipped", &skipped_count.to_string()),
                        ],
                    )
                } else {
                    tr_args(
                        "gui.git_import_partial",
                        &[
                            ("saved", &saved.to_string()),
                            ("skipped", &skipped_count.to_string()),
                            ("failed", &save_errors.len().to_string()),
                        ],
                    )
                };
                self.status_message = msg.clone();

                if let Some(ref mut dialog) = self.git_import_dialog {
                    dialog.error = None;
                    let mut detail = msg;
                    if !result.skipped.is_empty() {
                        detail.push_str("\n\n");
                        detail.push_str(tr("gui.git_import_skipped_header"));
                        for s in result.skipped.iter().take(10) {
                            detail.push_str(&format!("\n  {} — {}", s.path, s.reason));
                        }
                        if result.skipped.len() > 10 {
                            detail.push_str(&format!(
                                "\n  {}",
                                tr_args(
                                    "gui.git_import_more",
                                    &[("count", &(result.skipped.len() - 10).to_string())]
                                )
                            ));
                        }
                    }
                    if !save_errors.is_empty() {
                        detail.push_str("\n\n");
                        detail.push_str(tr("gui.git_import_save_errors"));
                        for e in save_errors.iter().take(5) {
                            detail.push_str(&format!("\n  {}", e));
                        }
                    }
                    dialog.result_message = Some(detail);
                    dialog.loading = false;
                }
            }
            Err(e) => {
                if let Some(ref mut dialog) = self.git_import_dialog {
                    dialog.error = Some(e.to_string());
                    dialog.loading = false;
                }
                self.status_message = tr_args("gui.git_import_error", &[("err", &e.to_string())]);
            }
        }
    }

    /// 選択中のパッチを削除する
    pub fn delete_selected_patch(&mut self) {
        let Some(ref patch_path) = self.selected_patch.clone() else {
            return;
        };
        let patch_path = patch_path.clone();

        if self.settings.patch_repo_path.is_empty() {
            return;
        }
        let repo = PatchRepository::new(&self.settings.patch_repo_path);
        match repo.delete(&patch_path) {
            Ok(()) => {
                self.selected_patch = None;
                self.preview_text = String::new();
                self.status_message = tr_args("gui.patch_deleted", &[("path", &patch_path)]);
                self.reload_patches();
            }
            Err(e) => {
                self.status_message = tr_args("gui.patch_delete_error", &[("err", &e.to_string())]);
            }
        }
    }
}

impl eframe::App for DriftPatchApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::ui::render(self, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_patch(target_file: &str) -> PatchFile {
        PatchFile {
            version: "1".to_string(),
            id: "test-id".to_string(),
            author: "test".to_string(),
            created_at: "2026-06-28T10:00:00+0900".to_string(),
            description: "desc".to_string(),
            target_file: target_file.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: driftpatch::patch::model::PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
            hunks: vec![],
        }
    }

    #[test]
    fn test_patches_for_open_file_includes_rename_old_path() {
        let mut app = DriftPatchApp::default();
        app.settings.work_dir = std::env::temp_dir().to_string_lossy().into_owned();
        app.file_path = Some(std::path::Path::new(&app.settings.work_dir).join("src/Old.java"));

        let mut rename_patch = dummy_patch("src/New.java");
        rename_patch.kind = PatchKind::Rename;
        rename_patch.old_path = Some("src/Old.java".to_string());
        rename_patch.verify_tokens = Some(vec![]);

        app.patches = vec![
            ("src/New.java/r.dpatch".to_string(), rename_patch),
            (
                "src/Other.java/o.dpatch".to_string(),
                dummy_patch("src/Other.java"),
            ),
        ];

        // 移動元ファイルを開いているとき、リネームパッチが一覧に出ること
        let filtered = app.patches_for_open_file();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "src/New.java/r.dpatch");
    }

    #[test]
    fn test_apply_selected_delete_patch_creates_bak_and_removes_file() {
        use driftpatch::lexer::profiles::JAVA;
        use driftpatch::patch::verify::significant_token_texts;

        let tmp = std::env::temp_dir().join(format!("driftpatch_gui_del_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let content = "class Doomed {\n    void a() {}\n}\n";
        let target = tmp.join("Doomed.java");
        std::fs::write(&target, content).unwrap();

        let mut patch = dummy_patch("Doomed.java");
        patch.kind = PatchKind::Delete;
        patch.verify_tokens = Some(significant_token_texts(content, &JAVA));

        let mut app = DriftPatchApp::default();
        app.file_path = Some(target.clone());
        app.original_text = content.to_string();
        app.edited_text = content.to_string();
        app.encoding = "UTF-8".to_string();
        app.patches = vec![("Doomed.java/d.dpatch".to_string(), patch)];
        app.selected_patch = Some("Doomed.java/d.dpatch".to_string());

        app.apply_selected_patch();

        assert!(
            app.status_message.contains("削除パッチ適用完了"),
            "ステータス: {}",
            app.status_message
        );

        // ファイルが削除され、.bak に元内容が残ること
        assert!(!target.exists(), "対象ファイルが削除されること");
        let bak = target.with_file_name("Doomed.java.bak");
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), content);

        // エディタ状態がクリアされること
        assert!(app.file_path.is_none());
        assert!(app.original_text.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_selected_delete_patch_blocks_on_drift() {
        use driftpatch::lexer::profiles::JAVA;
        use driftpatch::patch::verify::significant_token_texts;

        let tmp =
            std::env::temp_dir().join(format!("driftpatch_gui_drift_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        // パッチ記録時と異なる内容のファイル（ドリフト状態）
        let on_disk = "class Drifted { void extra() {} }\n";
        let target = tmp.join("Drifted.java");
        std::fs::write(&target, on_disk).unwrap();

        let mut patch = dummy_patch("Drifted.java");
        patch.kind = PatchKind::Delete;
        patch.verify_tokens = Some(significant_token_texts("class Drifted {}\n", &JAVA));

        let mut app = DriftPatchApp::default();
        app.file_path = Some(target.clone());
        app.original_text = on_disk.to_string();
        app.edited_text = on_disk.to_string();
        app.patches = vec![("Drifted.java/d.dpatch".to_string(), patch)];
        app.selected_patch = Some("Drifted.java/d.dpatch".to_string());

        app.apply_selected_patch();

        assert!(
            app.status_message.contains("パッチ適用エラー"),
            "ステータス: {}",
            app.status_message
        );
        assert!(target.exists(), "ドリフト検出時はファイルが残ること");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_patches_for_open_file_filters_by_target_file() {
        let mut app = DriftPatchApp::default();
        app.settings.work_dir = std::env::temp_dir().to_string_lossy().into_owned();
        app.file_path = Some(std::path::Path::new(&app.settings.work_dir).join("src/Foo.java"));
        app.patches = vec![
            (
                "src/Foo.java/a.dpatch".to_string(),
                dummy_patch("src/Foo.java"),
            ),
            (
                "src/Bar.java/b.dpatch".to_string(),
                dummy_patch("src/Bar.java"),
            ),
        ];

        let filtered = app.patches_for_open_file();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0, "src/Foo.java/a.dpatch");
    }

    #[test]
    fn test_apply_selected_patch_writes_file_and_backup() {
        use driftpatch::lexer::profiles::JAVA;
        use driftpatch::patch::context::ContextConfig;
        use driftpatch::patch::generator::generate_patch;

        let tmp = std::env::temp_dir().join(format!("driftpatch_apply_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let target = tmp.join("Foo.java");
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        std::fs::write(&target, orig).unwrap();

        let patch = generate_patch(
            orig,
            edit,
            &JAVA,
            "tester",
            "t",
            "Foo.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();

        let mut app = DriftPatchApp::default();
        app.file_path = Some(target.clone());
        app.original_text = orig.to_string();
        app.edited_text = orig.to_string();
        app.encoding = "UTF-8".to_string();
        app.patches = vec![("Foo.java/p.dpatch".to_string(), patch)];
        app.selected_patch = Some("Foo.java/p.dpatch".to_string());

        app.apply_selected_patch();

        assert!(
            app.status_message.contains("パッチ適用完了"),
            "ステータス: {}",
            app.status_message
        );

        // ファイル本体がパッチ済み内容に更新されていること
        let on_disk = std::fs::read_to_string(&target).unwrap();
        assert!(on_disk.contains("Objects.requireNonNull"));

        // .bak バックアップが適用前の元内容であること
        let bak = target.with_file_name("Foo.java.bak");
        assert!(
            bak.exists(),
            "バックアップが作成されていません: {}",
            bak.display()
        );
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), orig);

        // メモリ状態もパッチ済み内容に更新されていること
        assert_eq!(app.original_text, on_disk);
        assert_eq!(app.edited_text, on_disk);
        assert!(app.preview_text.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_apply_selected_patch_skips_backup_when_disabled() {
        use driftpatch::lexer::profiles::JAVA;
        use driftpatch::patch::context::ContextConfig;
        use driftpatch::patch::generator::generate_patch;

        let tmp = std::env::temp_dir().join(format!("driftpatch_nobak_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();

        let target = tmp.join("Bar.java");
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        std::fs::write(&target, orig).unwrap();

        let patch = generate_patch(
            orig,
            edit,
            &JAVA,
            "tester",
            "t",
            "Bar.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();

        let mut app = DriftPatchApp::default();
        app.settings.create_backup = false;
        app.file_path = Some(target.clone());
        app.original_text = orig.to_string();
        app.edited_text = orig.to_string();
        app.encoding = "UTF-8".to_string();
        app.patches = vec![("Bar.java/p.dpatch".to_string(), patch)];
        app.selected_patch = Some("Bar.java/p.dpatch".to_string());

        app.apply_selected_patch();

        assert!(
            app.status_message.contains("パッチ適用完了"),
            "ステータス: {}",
            app.status_message
        );
        assert!(
            !app.status_message.contains("バックアップ"),
            "バックアップ無効時はステータスにバックアップを含めない: {}",
            app.status_message
        );

        // ファイル本体はパッチ済み内容に更新されていること
        let on_disk = std::fs::read_to_string(&target).unwrap();
        assert!(on_disk.contains("Objects.requireNonNull"));

        // .bak は作成されないこと
        let bak = target.with_file_name("Bar.java.bak");
        assert!(
            !bak.exists(),
            "バックアップが作成されてしまいました: {}",
            bak.display()
        );

        assert_eq!(app.original_text, on_disk);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

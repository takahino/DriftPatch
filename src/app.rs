use std::path::{Path, PathBuf};

use driftpatch::encoding::read_file_auto;
use driftpatch::git_import::{generate_patches_from_commit, list_commits, CommitInfo};
use driftpatch::lexer::profiles::detect_profile;
use driftpatch::patch::context::ContextConfig;
use driftpatch::patch::file_ops::backup_path;
use driftpatch::patch::model::{PatchFile, PatchKind};
use driftpatch::patch::name_gen::generate_filename;
use driftpatch::patch::repository::PatchRepository;
use driftpatch::patch::verify::verify_significant_tokens;
use driftpatch::patch::{
    apply_patch, generate_patch, ApplyError, ApplyOptions, GeneratorError, PatchWorkspace,
    PlannedAction,
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
}

fn default_create_backup() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            patch_repo_path: String::new(),
            username: String::new(),
            work_dir: String::new(),
            git_repo_path: String::new(),
            create_backup: default_create_backup(),
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
            status_message: "ファイルを開いてください".to_string(),
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
                self.status_message = format!(
                    "開いたファイル: {} | 言語: {} | エンコード: {}",
                    filename, self.language, enc
                );
            }
            Err(e) => {
                self.status_message = format!("ファイルオープンエラー: {}", e);
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
                self.status_message = format!("パッチ一覧読み込みエラー: {}", e);
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
            return Err("work_dir が設定されていません（設定を確認してください）".to_string());
        }
        let work_path = std::path::Path::new(work_dir);
        let rel = file_path.strip_prefix(work_path).map_err(|_| {
            format!(
                "対象ファイルが work_dir 配下にありません: {}",
                file_path.display()
            )
        })?;
        Ok(rel.to_str().unwrap_or("").replace('\\', "/"))
    }

    /// パッチの target_file から絶対パスを解決する
    fn resolve_target_file(&self, patch: &PatchFile) -> Result<PathBuf, String> {
        if self.settings.work_dir.trim().is_empty() {
            return Err("work_dir が設定されていません".to_string());
        }
        if patch.target_file.is_empty() {
            return Err("パッチに target_file がありません".to_string());
        }
        Ok(std::path::Path::new(&self.settings.work_dir).join(&patch.target_file))
    }

    /// 元テキストと編集テキストからパッチを生成して保存する
    pub fn generate_and_save_patch(&mut self, description: &str) -> Result<(), String> {
        let Some(ref file_path) = self.file_path.clone() else {
            return Err("ファイルが開かれていません".to_string());
        };

        if self.settings.patch_repo_path.is_empty() {
            return Err(
                "パッチリポジトリパスが設定されていません（設定を確認してください）".to_string(),
            );
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
                        self.status_message = format!("パッチ保存: {}", saved_path.display());
                        self.reload_patches();
                        Ok(())
                    }
                    Err(e) => Err(format!("パッチ保存エラー: {}", e)),
                }
            }
            Err(GeneratorError::NoDiff) => Err("変更が見つかりませんでした".to_string()),
            Err(GeneratorError::NoMatch { hunk_index }) => Err(format!(
                "ハンク {} の適用箇所が見つかりませんでした",
                hunk_index
            )),
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
            self.status_message =
                "選択したパッチは現在開いているファイル向けではありません".to_string();
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
                            Ok(()) => {
                                "削除パッチ: 適用するとこのファイルは削除されます（内容検証 OK）"
                                    .to_string()
                            }
                            Err(m) => format!(
                                "削除パッチ: 内容がパッチ記録時と一致しません（ドリフト検出）: {}",
                                m
                            ),
                        }
                    }
                    None => "削除パッチ: verify_tokens がありません（不正なパッチ）".to_string(),
                };
                return;
            }
            PatchKind::Create => {
                // Create は空文字列への適用で作成される全文をプレビューする
                match apply_patch("", &patch, profile) {
                    Ok(text) => {
                        self.preview_text = text;
                        self.status_message = format!(
                            "新規作成パッチ: 適用すると {} が作成されます",
                            patch.target_file
                        );
                    }
                    Err(e) => {
                        self.preview_text = String::new();
                        self.status_message = format!("プレビュー失敗: {}", e);
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
                        self.status_message = format!(
                            "リネームパッチ: {} → {}",
                            patch.old_path.as_deref().unwrap_or("?"),
                            patch.target_file
                        );
                    }
                    Err(e) => {
                        self.preview_text = String::new();
                        self.status_message = format!("プレビュー失敗: {}", e);
                    }
                }
                return;
            }
            PatchKind::Modify => {}
        }

        match apply_patch(&self.original_text, &patch, profile) {
            Ok(result) => {
                self.preview_text = result;
                self.status_message = "プレビュー更新完了".to_string();
            }
            Err(ApplyError::NoMatch { hunk_index }) => {
                self.preview_text = String::new();
                self.status_message =
                    format!("適用失敗: ハンク {} の対象箇所が見つかりません", hunk_index);
            }
            Err(ApplyError::CountMismatch {
                hunk_index,
                expected,
                actual,
                ..
            }) => {
                self.preview_text = String::new();
                self.status_message = format!(
                    "適用失敗: ハンク {} の期待マッチ数 {} と実際のマッチ数 {} が一致しません（ドリフト検出）。",
                    hunk_index, expected, actual
                );
            }
            Err(ApplyError::OverlappingMatches { hunk_index }) => {
                self.preview_text = String::new();
                self.status_message = format!(
                    "適用失敗: ハンク {} の複数マッチの置換範囲が重なっています。",
                    hunk_index
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
                self.status_message =
                    "リネームパッチの適用には work_dir の設定が必要です".to_string();
                return;
            }
            let mut ws = PatchWorkspace::new(&work_dir);
            match ws.apply(&patch, &opts) {
                Ok(PlannedAction::Rename { from, to }) => {
                    // 移動先のファイルを開き直してエディタ状態を追従させる
                    let new_abs = std::path::Path::new(&work_dir)
                        .join(to.replace('/', std::path::MAIN_SEPARATOR_STR));
                    self.open_file(new_abs);
                    self.status_message = format!("リネーム適用完了: {} → {}", from, to);
                }
                Ok(action) => {
                    self.status_message = format!("リネームパッチ: {}", action.describe(false));
                }
                Err(e) => {
                    self.status_message = format!("パッチ適用エラー: {}", e);
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
                    format!(
                        "パッチ適用完了: {} に保存、バックアップ: {}",
                        file_path.display(),
                        backup_path(file_path).display()
                    )
                } else {
                    format!("パッチ適用完了: {} に保存", file_path.display())
                };
            }
            Ok(PlannedAction::Delete) => {
                // 対象ファイルが消えたためエディタを閉じる
                self.file_path = None;
                self.original_text = String::new();
                self.edited_text = String::new();
                self.preview_text = String::new();
                self.status_message = if self.settings.create_backup {
                    format!(
                        "削除パッチ適用完了: {} を削除、バックアップ: {}",
                        file_path.display(),
                        backup_path(file_path).display()
                    )
                } else {
                    format!("削除パッチ適用完了: {} を削除", file_path.display())
                };
            }
            Ok(action) => {
                self.status_message = format!("パッチ適用: {}", action.describe(false));
            }
            Err(e) => {
                self.status_message = format!("パッチ適用エラー: {}", e);
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
            self.status_message =
                "Git リポジトリパスまたは work_dir が設定されていません".to_string();
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
                self.status_message = format!("Git 履歴読み込みエラー: {}", e);
            }
        }
    }

    /// 指定コミットからパッチを生成してリポジトリに保存する
    pub fn import_from_commit(&mut self, commit_sha: &str, description: &str) {
        let Some(repo_path) = self.resolve_git_repo_path() else {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error =
                    Some("Git リポジトリパスまたは work_dir が設定されていません".to_string());
            }
            return;
        };

        if self.settings.patch_repo_path.is_empty() {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error = Some("パッチリポジトリパスが設定されていません".to_string());
            }
            return;
        }

        let work_dir = self.settings.work_dir.trim();
        if work_dir.is_empty() {
            if let Some(ref mut dialog) = self.git_import_dialog {
                dialog.error = Some("work_dir が設定されていません".to_string());
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
                    format!(
                        "Git 取り込み完了: {} 件保存, {} 件スキップ",
                        saved, skipped_count
                    )
                } else {
                    format!(
                        "Git 取り込み: {} 件保存, {} 件スキップ, {} 件保存失敗",
                        saved,
                        skipped_count,
                        save_errors.len()
                    )
                };
                self.status_message = msg.clone();

                if let Some(ref mut dialog) = self.git_import_dialog {
                    dialog.error = None;
                    let mut detail = msg;
                    if !result.skipped.is_empty() {
                        detail.push_str("\n\nスキップ:");
                        for s in result.skipped.iter().take(10) {
                            detail.push_str(&format!("\n  {} — {}", s.path, s.reason));
                        }
                        if result.skipped.len() > 10 {
                            detail
                                .push_str(&format!("\n  ... 他 {} 件", result.skipped.len() - 10));
                        }
                    }
                    if !save_errors.is_empty() {
                        detail.push_str("\n\n保存エラー:");
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
                self.status_message = format!("Git 取り込みエラー: {}", e);
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
                self.status_message = format!("削除: {}", patch_path);
                self.reload_patches();
            }
            Err(e) => {
                self.status_message = format!("削除エラー: {}", e);
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

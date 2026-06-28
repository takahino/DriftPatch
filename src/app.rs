use std::path::{Path, PathBuf};

use driftpatch::encoding::read_file_auto;
use driftpatch::git_import::{generate_patches_from_commit, list_commits, CommitInfo};
use driftpatch::lexer::profiles::detect_profile;
use driftpatch::patch::context::ContextConfig;
use driftpatch::patch::model::PatchFile;
use driftpatch::patch::name_gen::generate_filename;
use driftpatch::patch::repository::PatchRepository;
use driftpatch::patch::{apply_patch, generate_patch, ApplyError, GeneratorError};

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            patch_repo_path: String::new(),
            username: String::new(),
            work_dir: String::new(),
            git_repo_path: String::new(),
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
            .filter(|(_, patch)| patch.target_file == *rel)
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
            return Err("パッチリポジトリパスが設定されていません（設定を確認してください）".to_string());
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
            Err(GeneratorError::NotUnique { hunk_index, match_count }) => Err(format!(
                "ハンク {} のパターンが {} 箇所マッチしており、一意に特定できません。\nより詳細な変更前後のコードを指定してください。",
                hunk_index, match_count
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

        if self.resolve_target_file(&patch).ok().as_ref() != Some(file_path) {
            self.preview_text = String::new();
            self.status_message = "選択したパッチは現在開いているファイル向けではありません".to_string();
            return;
        }

        let profile = detect_profile(file_path);

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
            Err(ApplyError::AmbiguousMatch {
                hunk_index,
                match_count,
                ..
            }) => {
                self.preview_text = String::new();
                self.status_message = format!(
                    "適用失敗: ハンク {} が {} 箇所にマッチします。手動確認が必要です。",
                    hunk_index, match_count
                );
            }
        }
    }

    /// 選択中のパッチを元テキストに適用して original_text と edited_text を更新する
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
        let profile = detect_profile(file_path);

        match apply_patch(&self.original_text, &patch, profile) {
            Ok(result) => {
                self.original_text = result.clone();
                self.edited_text = result;
                self.preview_text = String::new();
                self.status_message = "パッチ適用完了".to_string();
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
                            detail.push_str(&format!(
                                "\n  ... 他 {} 件",
                                result.skipped.len() - 10
                            ));
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
            hunks: vec![],
        }
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
}

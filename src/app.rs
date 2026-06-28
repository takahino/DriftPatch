use std::path::PathBuf;

use crate::encoding::read_file_auto;
use crate::lexer::profiles::detect_profile;
use crate::patch::context::ContextConfig;
use crate::patch::model::PatchFile;
use crate::patch::name_gen::generate_filename;
use crate::patch::repository::PatchRepository;
use crate::patch::{apply_patch, generate_patch, ApplyError, GeneratorError};

/// 永続化する設定
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    /// patches/ リポジトリのベースパス
    pub patch_repo_path: String,
    /// パッチ生成時の作者名
    pub username: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            patch_repo_path: String::new(),
            username: String::new(),
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
    /// パッチ一覧
    pub patches: Vec<(String, PatchFile)>,
    /// 選択中のパッチインデックス
    pub selected_patch: Option<usize>,
    /// 設定
    pub settings: Settings,
    /// ステータスバーメッセージ
    pub status_message: String,
    /// 設定ウィンドウ表示フラグ
    pub show_settings: bool,
    /// パッチ生成ダイアログ
    pub generate_patch_dialog: Option<GeneratePatchDialog>,
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

                // パッチ一覧を更新
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
            }
            Err(e) => {
                self.status_message = format!("パッチ一覧読み込みエラー: {}", e);
            }
        }
    }

    /// 元テキストと編集テキストからパッチを生成して保存する
    pub fn generate_and_save_patch(&mut self, description: &str) -> Result<(), String> {
        let Some(ref file_path) = self.file_path.clone() else {
            return Err("ファイルが開かれていません".to_string());
        };

        if self.settings.patch_repo_path.is_empty() {
            return Err("パッチリポジトリパスが設定されていません（設定を確認してください）".to_string());
        }

        let profile = detect_profile(file_path);
        let config = ContextConfig::default();

        match generate_patch(
            &self.original_text,
            &self.edited_text,
            profile,
            &self.settings.username,
            description,
            file_path.to_str().unwrap_or(""),
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
        let Some(idx) = self.selected_patch else {
            self.preview_text = String::new();
            return;
        };
        let Some((_, patch)) = self.patches.get(idx) else {
            self.preview_text = String::new();
            return;
        };

        let Some(ref file_path) = self.file_path.clone() else {
            return;
        };

        let profile = detect_profile(file_path);
        let patch = patch.clone();

        match apply_patch(&self.original_text, &patch, profile) {
            Ok(result) => {
                self.preview_text = result;
                self.status_message = "プレビュー更新完了".to_string();
            }
            Err(ApplyError::NoMatch { hunk_index }) => {
                self.preview_text = String::new();
                self.status_message = format!("適用失敗: ハンク {} の対象箇所が見つかりません", hunk_index);
            }
            Err(ApplyError::AmbiguousMatch { hunk_index, match_count, .. }) => {
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
        let Some(idx) = self.selected_patch else { return; };
        let Some((_, patch)) = self.patches.get(idx) else { return; };

        let Some(ref file_path) = self.file_path.clone() else { return; };
        let profile = detect_profile(file_path);
        let patch = patch.clone();

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

    /// 選択中のパッチを削除する
    pub fn delete_selected_patch(&mut self) {
        let Some(idx) = self.selected_patch else { return; };
        let Some((filename, _)) = self.patches.get(idx) else { return; };
        let filename = filename.clone();

        if self.settings.patch_repo_path.is_empty() { return; }
        let repo = PatchRepository::new(&self.settings.patch_repo_path);
        match repo.delete(&filename) {
            Ok(()) => {
                self.selected_patch = None;
                self.preview_text = String::new();
                self.status_message = format!("削除: {}", filename);
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

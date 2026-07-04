use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::encoding::{read_file_auto, write_file_auto};
use crate::lexer::profiles::detect_profile;
use crate::patch::applier::{apply_patch, ApplyError};
use crate::patch::model::{PatchFile, PatchKind};
use crate::patch::verify::{significant_token_texts, verify_significant_tokens, VerifyMismatch};

/// ファイル操作を伴うパッチ適用のエラー
#[derive(Debug)]
pub enum FileOpError {
    /// テキスト適用エラー（NoMatch / CountMismatch などのドリフト検出を含む）
    Apply(ApplyError),
    Io(String),
    /// 対象ファイルが存在しない。deleted_earlier は同一バッチ内の先行パッチで削除済みの場合
    TargetNotFound { path: PathBuf, deleted_earlier: bool },
    /// Create / Rename 先に異なる内容のファイルが既に存在する
    FileAlreadyExists(PathBuf),
    /// 削除パッチの内容検証に失敗（ドリフト検出により削除を中止）
    DeleteVerificationFailed { path: PathBuf, mismatch: VerifyMismatch },
    /// リネームパッチの移動前内容検証に失敗
    RenameVerificationFailed { path: PathBuf, mismatch: VerifyMismatch },
    /// kind と付随フィールドの整合性エラー
    InvalidPatch(String),
}

impl std::fmt::Display for FileOpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOpError::Apply(e) => write!(f, "{}", e),
            FileOpError::Io(msg) => write!(f, "{}", msg),
            FileOpError::TargetNotFound { path, deleted_earlier } => {
                if *deleted_earlier {
                    write!(
                        f,
                        "対象ファイルは先行パッチにより削除済みです: {}",
                        path.display()
                    )
                } else {
                    write!(f, "対象ファイルが見つかりません: {}", path.display())
                }
            }
            FileOpError::FileAlreadyExists(path) => write!(
                f,
                "作成先に異なる内容のファイルが既に存在します: {}",
                path.display()
            ),
            FileOpError::DeleteVerificationFailed { path, mismatch } => write!(
                f,
                "削除を中止しました。ファイル内容がパッチ記録時と一致しません（ドリフト検出）: {} ({})",
                path.display(),
                mismatch
            ),
            FileOpError::RenameVerificationFailed { path, mismatch } => write!(
                f,
                "リネームを中止しました。移動前ファイルの内容がパッチ記録時と一致しません（ドリフト検出）: {} ({})",
                path.display(),
                mismatch
            ),
            FileOpError::InvalidPatch(msg) => write!(f, "パッチが不正: {}", msg),
        }
    }
}

/// 実行された（dry-run では予定される）操作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlannedAction {
    Modify,
    Create,
    Delete,
    Rename { from: String, to: String },
    /// 冪等ケース: 既に適用済みの状態だったため何もしなかった
    AlreadyApplied,
}

impl PlannedAction {
    /// レポートの「操作」列用の識別子
    pub fn kind_str(&self) -> &'static str {
        match self {
            PlannedAction::Modify => "modify",
            PlannedAction::Create => "create",
            PlannedAction::Delete => "delete",
            PlannedAction::Rename { .. } => "rename",
            PlannedAction::AlreadyApplied => "already_applied",
        }
    }

    /// レポート・ステータス表示用メッセージ
    pub fn describe(&self, dry_run: bool) -> String {
        let body = match self {
            PlannedAction::Modify => "適用成功".to_string(),
            PlannedAction::Create => "ファイル作成".to_string(),
            PlannedAction::Delete => "ファイル削除".to_string(),
            PlannedAction::Rename { from, to } => format!("リネーム: {} → {}", from, to),
            PlannedAction::AlreadyApplied => "適用済み（変更なし）".to_string(),
        };
        if dry_run {
            format!("[dry-run] {}", body)
        } else {
            body
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ApplyOptions {
    /// true ならディスクへの変更を一切行わず、適用可否の判定のみ行う
    pub dry_run: bool,
    /// 変更・削除・リネーム前に .bak バックアップを作成する
    pub create_backup: bool,
}

/// メモリ上のファイル状態。dry-run や同一ファイルへの複数パッチ逐次適用のために
/// ディスクとは独立に保持する。
enum FileState {
    Present { text: String, encoding: String },
    Deleted,
}

/// lookup の結果（借用を避けるため所有値で返す）
enum Lookup {
    Present(String, String),
    DeletedEarlier,
    Missing,
}

/// work_dir を基準としたパッチ適用ワークスペース。
/// kind（Modify / Create / Delete / Rename）に応じたファイル操作を担い、
/// dry-run ではキャッシュのみ更新してディスクを触らない。
pub struct PatchWorkspace {
    work_dir: PathBuf,
    cache: HashMap<PathBuf, FileState>,
}

impl PatchWorkspace {
    pub fn new(work_dir: impl Into<PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
            cache: HashMap::new(),
        }
    }

    /// work_dir 相対パス（`/` 区切り）を絶対パスに解決する
    fn resolve(&self, rel: &str) -> PathBuf {
        self.work_dir
            .join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))
    }

    /// 1 パッチを kind に応じて適用する。target_file は work_dir 相対で解決する。
    pub fn apply(
        &mut self,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        self.apply_at(None, patch, opts)
    }

    /// GUI など、対象の絶対パスが確定している場合の適用。
    /// explicit_target は Modify / Create / Delete でのみ有効（Rename は旧・新の
    /// 2 パスが必要なため work_dir 基準でのみ解決する）。
    pub fn apply_at(
        &mut self,
        explicit_target: Option<&Path>,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        patch.validate().map_err(FileOpError::InvalidPatch)?;

        let target_path = explicit_target
            .map(Path::to_path_buf)
            .unwrap_or_else(|| self.resolve(&patch.target_file));

        match patch.kind {
            PatchKind::Modify => self.apply_modify(target_path, patch, opts),
            PatchKind::Create => self.apply_create(target_path, patch, opts),
            PatchKind::Delete => self.apply_delete(target_path, patch, opts),
            PatchKind::Rename => self.apply_rename(patch, opts),
        }
    }

    /// 適用後のメモリ上テキスト（GUI がエディタ状態を更新するために使う）
    pub fn cached_text(&self, target_file: &str) -> Option<&str> {
        self.cached_text_at(&self.resolve(target_file))
    }

    /// 絶対パス指定版（apply_at で explicit_target を使った場合はこちらで参照する）
    pub fn cached_text_at(&self, path: &Path) -> Option<&str> {
        match self.cache.get(path) {
            Some(FileState::Present { text, .. }) => Some(text.as_str()),
            _ => None,
        }
    }

    fn lookup(&mut self, path: &Path) -> Result<Lookup, FileOpError> {
        if let Some(state) = self.cache.get(path) {
            return Ok(match state {
                FileState::Present { text, encoding } => {
                    Lookup::Present(text.clone(), encoding.clone())
                }
                FileState::Deleted => Lookup::DeletedEarlier,
            });
        }
        if !path.exists() {
            return Ok(Lookup::Missing);
        }
        let (text, enc) = read_file_auto(path)
            .map_err(|e| FileOpError::Io(format!("ファイル読込エラー: {}: {}", path.display(), e)))?;
        self.cache.insert(
            path.to_path_buf(),
            FileState::Present {
                text: text.clone(),
                encoding: enc.clone(),
            },
        );
        Ok(Lookup::Present(text, enc))
    }

    fn apply_modify(
        &mut self,
        target_path: PathBuf,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        let (text, detected_enc) = match self.lookup(&target_path)? {
            Lookup::Present(t, e) => (t, e),
            Lookup::DeletedEarlier => {
                return Err(FileOpError::TargetNotFound {
                    path: target_path,
                    deleted_earlier: true,
                })
            }
            Lookup::Missing => {
                return Err(FileOpError::TargetNotFound {
                    path: target_path,
                    deleted_earlier: false,
                })
            }
        };

        let profile = detect_profile(&target_path);
        let result = apply_patch(&text, patch, profile).map_err(FileOpError::Apply)?;
        let encoding = choose_encoding(patch, detected_enc);

        if !opts.dry_run {
            if opts.create_backup {
                create_backup_file(&target_path)?;
            }
            write_file_auto(&target_path, &result, &encoding).map_err(|e| {
                FileOpError::Io(format!(
                    "ファイル書込エラー: {}: {}",
                    target_path.display(),
                    e
                ))
            })?;
        }

        self.cache.insert(
            target_path,
            FileState::Present {
                text: result,
                encoding,
            },
        );
        Ok(PlannedAction::Modify)
    }

    fn apply_create(
        &mut self,
        target_path: PathBuf,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        let profile = detect_profile(&target_path);
        // Create パッチは「空文字列との diff」なので空テキストへの適用で全文が得られる
        let new_text = apply_patch("", patch, profile).map_err(FileOpError::Apply)?;

        if let Lookup::Present(existing, _) = self.lookup(&target_path)? {
            // 既に存在する場合: 実質同一内容なら冪等成功、異なればドリフト扱い
            if significant_token_texts(&existing, profile)
                == significant_token_texts(&new_text, profile)
            {
                return Ok(PlannedAction::AlreadyApplied);
            }
            return Err(FileOpError::FileAlreadyExists(target_path));
        }

        let encoding = if patch.encoding.is_empty() {
            "UTF-8".to_string()
        } else {
            patch.encoding.clone()
        };

        if !opts.dry_run {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    FileOpError::Io(format!(
                        "ディレクトリ作成エラー: {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            write_file_auto(&target_path, &new_text, &encoding).map_err(|e| {
                FileOpError::Io(format!(
                    "ファイル書込エラー: {}: {}",
                    target_path.display(),
                    e
                ))
            })?;
        }

        self.cache.insert(
            target_path,
            FileState::Present {
                text: new_text,
                encoding,
            },
        );
        Ok(PlannedAction::Create)
    }

    fn apply_delete(
        &mut self,
        target_path: PathBuf,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        let (text, _) = match self.lookup(&target_path)? {
            Lookup::Present(t, e) => (t, e),
            Lookup::DeletedEarlier => {
                return Err(FileOpError::TargetNotFound {
                    path: target_path,
                    deleted_earlier: true,
                })
            }
            Lookup::Missing => {
                return Err(FileOpError::TargetNotFound {
                    path: target_path,
                    deleted_earlier: false,
                })
            }
        };

        let profile = detect_profile(&target_path);
        let expected = patch
            .verify_tokens
            .as_ref()
            .ok_or_else(|| FileOpError::InvalidPatch("削除パッチに verify_tokens がありません".to_string()))?;

        // 現物がパッチ記録時の内容と一致する場合のみ削除する（誤削除防止）
        verify_significant_tokens(&text, profile, expected).map_err(|mismatch| {
            FileOpError::DeleteVerificationFailed {
                path: target_path.clone(),
                mismatch,
            }
        })?;

        if !opts.dry_run {
            if opts.create_backup {
                create_backup_file(&target_path)?;
            }
            fs::remove_file(&target_path).map_err(|e| {
                FileOpError::Io(format!(
                    "ファイル削除エラー: {}: {}",
                    target_path.display(),
                    e
                ))
            })?;
        }

        self.cache.insert(target_path, FileState::Deleted);
        Ok(PlannedAction::Delete)
    }

    fn apply_rename(
        &mut self,
        patch: &PatchFile,
        opts: &ApplyOptions,
    ) -> Result<PlannedAction, FileOpError> {
        let old_rel = patch
            .old_path
            .as_ref()
            .ok_or_else(|| FileOpError::InvalidPatch("リネームパッチに old_path がありません".to_string()))?;
        let old_path = self.resolve(old_rel);
        let new_path = self.resolve(&patch.target_file);
        let profile = detect_profile(Path::new(&patch.target_file));
        // NTFS は case-insensitive のため、大文字小文字のみのリネームでは
        // 新パスの存在チェックが自分自身にヒットしてしまう。同一視して除外する。
        let case_only_rename =
            old_rel.eq_ignore_ascii_case(&patch.target_file) && old_rel != &patch.target_file;

        let old_state = self.lookup(&old_path)?;

        let (old_text, detected_enc) = match old_state {
            Lookup::Present(t, e) => (t, e),
            other => {
                // 冪等性: 旧ファイルが無くても、新ファイルが期待内容で存在すれば
                // 適用済みとみなす（純リネームのみ判定可能）
                if patch.hunks.is_empty() {
                    if let Lookup::Present(new_text, _) = self.lookup(&new_path)? {
                        let expected = patch.verify_tokens.as_ref().ok_or_else(|| {
                            FileOpError::InvalidPatch(
                                "リネームパッチに verify_tokens がありません".to_string(),
                            )
                        })?;
                        if verify_significant_tokens(&new_text, profile, expected).is_ok() {
                            return Ok(PlannedAction::AlreadyApplied);
                        }
                    }
                }
                return Err(FileOpError::TargetNotFound {
                    path: old_path,
                    deleted_earlier: matches!(other, Lookup::DeletedEarlier),
                });
            }
        };

        // 検証とハンク適用をメモリ上で完了させてからファイル操作を行い、
        // 途中失敗で中途半端な状態（移動済みだが編集失敗など）を残さない
        let new_text = if patch.hunks.is_empty() {
            let expected = patch.verify_tokens.as_ref().ok_or_else(|| {
                FileOpError::InvalidPatch("リネームパッチに verify_tokens がありません".to_string())
            })?;
            verify_significant_tokens(&old_text, profile, expected).map_err(|mismatch| {
                FileOpError::RenameVerificationFailed {
                    path: old_path.clone(),
                    mismatch,
                }
            })?;
            old_text
        } else {
            apply_patch(&old_text, patch, profile).map_err(FileOpError::Apply)?
        };

        // Windows の fs::rename は既存ファイルを黙って置換するため、事前に衝突を検出する
        if !case_only_rename {
            if let Lookup::Present(..) = self.lookup(&new_path)? {
                return Err(FileOpError::FileAlreadyExists(new_path));
            }
        }

        let encoding = choose_encoding(patch, detected_enc);

        if !opts.dry_run {
            if opts.create_backup {
                create_backup_file(&old_path)?;
            }
            if let Some(parent) = new_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    FileOpError::Io(format!(
                        "ディレクトリ作成エラー: {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            fs::rename(&old_path, &new_path).map_err(|e| {
                FileOpError::Io(format!(
                    "リネームエラー: {} → {}: {}",
                    old_path.display(),
                    new_path.display(),
                    e
                ))
            })?;
            if !patch.hunks.is_empty() {
                write_file_auto(&new_path, &new_text, &encoding).map_err(|e| {
                    FileOpError::Io(format!(
                        "ファイル書込エラー: {}: {}",
                        new_path.display(),
                        e
                    ))
                })?;
            }
        }

        if !case_only_rename {
            self.cache.insert(old_path, FileState::Deleted);
        }
        self.cache.insert(
            new_path,
            FileState::Present {
                text: new_text,
                encoding,
            },
        );
        Ok(PlannedAction::Rename {
            from: old_rel.clone(),
            to: patch.target_file.clone(),
        })
    }
}

/// パッチ側の指定があればそれを、なければ検出したエンコーディングを使う
fn choose_encoding(patch: &PatchFile, detected: String) -> String {
    if patch.encoding.is_empty() {
        detected
    } else {
        patch.encoding.clone()
    }
}

/// 適用前のバックアップファイルパスを返す（Foo.java -> Foo.java.bak）
pub fn backup_path(file_path: &Path) -> PathBuf {
    let mut name = file_path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(".bak");
    file_path.with_file_name(name)
}

fn create_backup_file(path: &Path) -> Result<PathBuf, FileOpError> {
    let bak = backup_path(path);
    fs::copy(path, &bak).map_err(|e| {
        FileOpError::Io(format!("バックアップ作成失敗: {}: {}", bak.display(), e))
    })?;
    Ok(bak)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;
    use crate::patch::context::ContextConfig;
    use crate::patch::generator::generate_patch;
    use crate::patch::model::PATCH_FORMAT_VERSION;

    fn temp_workdir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("driftpatch_{}_{}", prefix, uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn create_patch_for(target_file: &str, content: &str) -> PatchFile {
        let mut patch = generate_patch(
            "",
            content,
            &JAVA,
            "tester",
            "create test",
            target_file,
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        patch.kind = PatchKind::Create;
        patch
    }

    fn delete_patch_for(target_file: &str, recorded_content: &str) -> PatchFile {
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: "20260704-delete-test0000".to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T10:00:00+0900".to_string(),
            description: "delete test".to_string(),
            target_file: target_file.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Delete,
            old_path: None,
            verify_tokens: Some(significant_token_texts(recorded_content, &JAVA)),
            hunks: vec![],
        }
    }

    fn pure_rename_patch(old_path: &str, new_path: &str, recorded_content: &str) -> PatchFile {
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: "20260704-rename-test0000".to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T10:00:00+0900".to_string(),
            description: "rename test".to_string(),
            target_file: new_path.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Rename,
            old_path: Some(old_path.to_string()),
            verify_tokens: Some(significant_token_texts(recorded_content, &JAVA)),
            hunks: vec![],
        }
    }

    fn no_backup() -> ApplyOptions {
        ApplyOptions {
            dry_run: false,
            create_backup: false,
        }
    }

    #[test]
    fn test_create_writes_file_and_parent_dirs() {
        let work = temp_workdir("fops_create");
        let content = "class NewFile {\n    void a() {}\n}\n";
        let patch = create_patch_for("src/sub/NewFile.java", content);

        let mut ws = PatchWorkspace::new(&work);
        let action = ws.apply(&patch, &no_backup()).unwrap();
        assert_eq!(action, PlannedAction::Create);

        let created = work.join("src").join("sub").join("NewFile.java");
        assert_eq!(fs::read_to_string(&created).unwrap(), content);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_create_already_applied_when_content_matches() {
        let work = temp_workdir("fops_create_idem");
        let content = "class Same {}\n";
        let target = work.join("Same.java");
        // 空白差はあるが significant token は同一
        fs::write(&target, "class  Same  {}\n").unwrap();

        let patch = create_patch_for("Same.java", content);
        let mut ws = PatchWorkspace::new(&work);
        let action = ws.apply(&patch, &no_backup()).unwrap();
        assert_eq!(action, PlannedAction::AlreadyApplied);
        // 既存ファイルは書き換えられないこと
        assert_eq!(fs::read_to_string(&target).unwrap(), "class  Same  {}\n");

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_create_fails_when_different_content_exists() {
        let work = temp_workdir("fops_create_conflict");
        let target = work.join("Conflict.java");
        fs::write(&target, "class Other {}\n").unwrap();

        let patch = create_patch_for("Conflict.java", "class Conflict {}\n");
        let mut ws = PatchWorkspace::new(&work);
        let result = ws.apply(&patch, &no_backup());
        assert!(matches!(result, Err(FileOpError::FileAlreadyExists(_))));
        assert_eq!(fs::read_to_string(&target).unwrap(), "class Other {}\n");

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_delete_removes_file_after_verification() {
        let work = temp_workdir("fops_delete");
        let content = "class Legacy {\n    void a() {}\n}\n";
        let target = work.join("Legacy.java");
        fs::write(&target, content).unwrap();

        let patch = delete_patch_for("Legacy.java", content);
        let mut ws = PatchWorkspace::new(&work);
        let action = ws.apply(&patch, &no_backup()).unwrap();
        assert_eq!(action, PlannedAction::Delete);
        assert!(!target.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_delete_allows_whitespace_drift() {
        // インデント・改行コードの差だけなら削除できること
        let work = temp_workdir("fops_delete_ws");
        let recorded = "class Legacy {\n    void a() {}\n}\n";
        let target = work.join("Legacy.java");
        fs::write(&target, "class Legacy {\r\n\tvoid a() {}\r\n}\r\n").unwrap();

        let patch = delete_patch_for("Legacy.java", recorded);
        let mut ws = PatchWorkspace::new(&work);
        assert_eq!(ws.apply(&patch, &no_backup()).unwrap(), PlannedAction::Delete);
        assert!(!target.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_delete_fails_on_drifted_content() {
        let work = temp_workdir("fops_delete_drift");
        let target = work.join("Drifted.java");
        fs::write(&target, "class Drifted { void extra() {} }\n").unwrap();

        let patch = delete_patch_for("Drifted.java", "class Drifted {}\n");
        let mut ws = PatchWorkspace::new(&work);
        let result = ws.apply(&patch, &no_backup());
        assert!(matches!(
            result,
            Err(FileOpError::DeleteVerificationFailed { .. })
        ));
        // ドリフト検出時はファイルが残ること
        assert!(target.exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_delete_creates_backup_when_enabled() {
        let work = temp_workdir("fops_delete_bak");
        let content = "class Bak {}\n";
        let target = work.join("Bak.java");
        fs::write(&target, content).unwrap();

        let patch = delete_patch_for("Bak.java", content);
        let mut ws = PatchWorkspace::new(&work);
        let opts = ApplyOptions {
            dry_run: false,
            create_backup: true,
        };
        ws.apply(&patch, &opts).unwrap();

        assert!(!target.exists());
        let bak = work.join("Bak.java.bak");
        assert_eq!(fs::read_to_string(&bak).unwrap(), content);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_rename_pure_moves_file() {
        let work = temp_workdir("fops_rename");
        let content = "class Moved {}\n";
        let old = work.join("Old.java");
        fs::write(&old, content).unwrap();

        let patch = pure_rename_patch("Old.java", "sub/New.java", content);
        let mut ws = PatchWorkspace::new(&work);
        let action = ws.apply(&patch, &no_backup()).unwrap();
        assert_eq!(
            action,
            PlannedAction::Rename {
                from: "Old.java".to_string(),
                to: "sub/New.java".to_string()
            }
        );
        assert!(!old.exists());
        assert_eq!(
            fs::read_to_string(work.join("sub").join("New.java")).unwrap(),
            content
        );

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_rename_with_edit_applies_hunks() {
        let work = temp_workdir("fops_rename_edit");
        let before = "class Renamed {\n    void a() {}\n}\n";
        let after = "class Renamed {\n    void a() { System.out.println(1); }\n}\n";
        let old = work.join("Before.java");
        fs::write(&old, before).unwrap();

        let mut patch = generate_patch(
            before,
            after,
            &JAVA,
            "tester",
            "rename edit",
            "After.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        patch.kind = PatchKind::Rename;
        patch.old_path = Some("Before.java".to_string());

        let mut ws = PatchWorkspace::new(&work);
        let action = ws.apply(&patch, &no_backup()).unwrap();
        assert!(matches!(action, PlannedAction::Rename { .. }));
        assert!(!old.exists());
        let moved = fs::read_to_string(work.join("After.java")).unwrap();
        assert!(moved.contains("System.out.println"));

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_rename_idempotent_when_new_exists() {
        let work = temp_workdir("fops_rename_idem");
        let content = "class Done {}\n";
        // 旧ファイルは無く、新ファイルが期待内容で存在する（適用済み状態）
        fs::write(work.join("New.java"), content).unwrap();

        let patch = pure_rename_patch("Old.java", "New.java", content);
        let mut ws = PatchWorkspace::new(&work);
        assert_eq!(
            ws.apply(&patch, &no_backup()).unwrap(),
            PlannedAction::AlreadyApplied
        );

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_rename_fails_verification_on_drift() {
        let work = temp_workdir("fops_rename_drift");
        fs::write(work.join("Old.java"), "class Old { void extra() {} }\n").unwrap();

        let patch = pure_rename_patch("Old.java", "New.java", "class Old {}\n");
        let mut ws = PatchWorkspace::new(&work);
        let result = ws.apply(&patch, &no_backup());
        assert!(matches!(
            result,
            Err(FileOpError::RenameVerificationFailed { .. })
        ));
        assert!(work.join("Old.java").exists());
        assert!(!work.join("New.java").exists());

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_rename_fails_when_target_occupied() {
        let work = temp_workdir("fops_rename_occupied");
        let content = "class Occupied {}\n";
        fs::write(work.join("Old.java"), content).unwrap();
        fs::write(work.join("New.java"), "class Different {}\n").unwrap();

        let patch = pure_rename_patch("Old.java", "New.java", content);
        let mut ws = PatchWorkspace::new(&work);
        let result = ws.apply(&patch, &no_backup());
        assert!(matches!(result, Err(FileOpError::FileAlreadyExists(_))));
        // どちらのファイルも変更されないこと
        assert_eq!(fs::read_to_string(work.join("Old.java")).unwrap(), content);
        assert_eq!(
            fs::read_to_string(work.join("New.java")).unwrap(),
            "class Different {}\n"
        );

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_dry_run_makes_no_disk_changes() {
        let work = temp_workdir("fops_dryrun");
        let modify_orig = "void foo() {\n    return null;\n}\n";
        let modify_edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let delete_content = "class ToDelete {}\n";
        let rename_content = "class ToMove {}\n";

        fs::write(work.join("Mod.java"), modify_orig).unwrap();
        fs::write(work.join("Del.java"), delete_content).unwrap();
        fs::write(work.join("Mov.java"), rename_content).unwrap();

        let modify_patch = generate_patch(
            modify_orig,
            modify_edit,
            &JAVA,
            "tester",
            "mod",
            "Mod.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();
        let create_patch = create_patch_for("Created.java", "class Created {}\n");
        let delete_patch = delete_patch_for("Del.java", delete_content);
        let rename_patch = pure_rename_patch("Mov.java", "Moved.java", rename_content);

        let opts = ApplyOptions {
            dry_run: true,
            create_backup: true, // dry-run では .bak も作られないこと
        };
        let mut ws = PatchWorkspace::new(&work);
        assert_eq!(ws.apply(&modify_patch, &opts).unwrap(), PlannedAction::Modify);
        assert_eq!(ws.apply(&create_patch, &opts).unwrap(), PlannedAction::Create);
        assert_eq!(ws.apply(&delete_patch, &opts).unwrap(), PlannedAction::Delete);
        assert!(matches!(
            ws.apply(&rename_patch, &opts).unwrap(),
            PlannedAction::Rename { .. }
        ));

        // ディスクが完全に無変更であること
        assert_eq!(fs::read_to_string(work.join("Mod.java")).unwrap(), modify_orig);
        assert_eq!(fs::read_to_string(work.join("Del.java")).unwrap(), delete_content);
        assert_eq!(fs::read_to_string(work.join("Mov.java")).unwrap(), rename_content);
        assert!(!work.join("Created.java").exists());
        assert!(!work.join("Moved.java").exists());
        let entries: Vec<_> = fs::read_dir(&work).unwrap().collect();
        assert_eq!(entries.len(), 3, ".bak 等の余計なファイルが作られないこと");

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_dry_run_cache_sequencing() {
        // dry-run でも同一ファイルへの後続パッチが正しく逐次判定されること
        let work = temp_workdir("fops_dryrun_seq");
        let orig = "void foo() {\n    return null;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step1 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 1;\n}\n";
        let step2 = "void foo() {\n    return 0;\n}\nvoid bar() {\n    return 2;\n}\n";
        fs::write(work.join("Seq.java"), orig).unwrap();

        let config = ContextConfig::default();
        let p1 = generate_patch(orig, step1, &JAVA, "t", "s1", "Seq.java", "UTF-8", &config).unwrap();
        let p2 = generate_patch(step1, step2, &JAVA, "t", "s2", "Seq.java", "UTF-8", &config).unwrap();

        let opts = ApplyOptions {
            dry_run: true,
            create_backup: false,
        };
        let mut ws = PatchWorkspace::new(&work);
        assert!(ws.apply(&p1, &opts).is_ok());
        // p2 は p1 適用後の内容にしかマッチしない → cache が更新されていれば成功する
        assert!(ws.apply(&p2, &opts).is_ok());
        // ディスクは無変更
        assert_eq!(fs::read_to_string(work.join("Seq.java")).unwrap(), orig);

        let _ = fs::remove_dir_all(&work);
    }

    #[test]
    fn test_delete_then_modify_same_file_fails() {
        let work = temp_workdir("fops_del_then_mod");
        let content = "void foo() {\n    return null;\n}\n";
        fs::write(work.join("Gone.java"), content).unwrap();

        let delete_patch = delete_patch_for("Gone.java", content);
        let modify_patch = generate_patch(
            content,
            "void foo() {\n    return 0;\n}\n",
            &JAVA,
            "t",
            "m",
            "Gone.java",
            "UTF-8",
            &ContextConfig::default(),
        )
        .unwrap();

        let mut ws = PatchWorkspace::new(&work);
        ws.apply(&delete_patch, &no_backup()).unwrap();
        let result = ws.apply(&modify_patch, &no_backup());
        assert!(matches!(
            result,
            Err(FileOpError::TargetNotFound {
                deleted_earlier: true,
                ..
            })
        ));

        let _ = fs::remove_dir_all(&work);
    }
}

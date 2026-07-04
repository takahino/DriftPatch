use std::fs;
use std::path::{Path, PathBuf};

use crate::patch::model::{PatchFile, SUPPORTED_VERSIONS};

#[derive(Debug)]
pub enum RepoError {
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidPath(String),
    /// フォーマットバージョンが未対応（新しいバージョンのパッチを旧コードが
    /// 「空の Modify」等と誤認して no-op 成功する事故を防ぐ）
    UnsupportedVersion(String),
    /// kind と付随フィールドの整合性エラー
    InvalidPatch(String),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::Io(e) => write!(f, "I/Oエラー: {}", e),
            RepoError::Json(e) => write!(f, "JSONエラー: {}", e),
            RepoError::InvalidPath(p) => write!(f, "パスが無効: {}", p),
            RepoError::UnsupportedVersion(v) => write!(
                f,
                "未対応のパッチフォーマットバージョンです: {}（新しい DriftPatch で作成された可能性があります）",
                v
            ),
            RepoError::InvalidPatch(msg) => write!(f, "パッチが不正: {}", msg),
        }
    }
}

impl From<std::io::Error> for RepoError {
    fn from(e: std::io::Error) -> Self {
        RepoError::Io(e)
    }
}

impl From<serde_json::Error> for RepoError {
    fn from(e: serde_json::Error) -> Self {
        RepoError::Json(e)
    }
}

/// パッチリポジトリの読み書きを担当する。
/// ツール自体は git コマンドを実行せず、ファイルの読み書きのみを行う。
pub struct PatchRepository {
    pub repo_path: PathBuf,
}

impl PatchRepository {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// patches/ ディレクトリのパスを返す
    pub fn patches_dir(&self) -> PathBuf {
        self.repo_path.join("patches")
    }

    /// target_file 配下の保存ディレクトリを返す
    fn target_dir(&self, target_file: &str) -> PathBuf {
        self.patches_dir()
            .join(normalize_path_separators(target_file))
    }

    /// パッチを保存する。`patches/<target_file>/<filename>` に配置する。
    pub fn save(&self, patch: &PatchFile, filename: &str) -> Result<PathBuf, RepoError> {
        if patch.target_file.is_empty() {
            return Err(RepoError::InvalidPath("target_file が空です".to_string()));
        }
        // 不正なパッチ（例: verify_tokens のない削除パッチ）の保存を防ぐ
        patch.validate().map_err(RepoError::InvalidPatch)?;

        let dir = self.target_dir(&patch.target_file);
        fs::create_dir_all(&dir)?;

        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(patch)?;
        fs::write(&path, json.as_bytes())?;
        Ok(path)
    }

    /// patches/ 以下の .dpatch を再帰的に列挙する。
    /// 戻り値の文字列は patches/ からの相対パス（`/` 区切り）。
    pub fn list(&self) -> Result<Vec<(String, PatchFile)>, RepoError> {
        let dir = self.patches_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut patches = Vec::new();
        collect_patches(&dir, &dir, &mut patches)?;

        patches.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(patches)
    }

    /// パッチディレクトリ直下の .dpatch を再帰列挙する（バッチ適用用）。
    /// `patch_dir` が patches/ そのものでも repo ルートでも動作する。
    pub fn list_from_dir(patch_dir: &Path) -> Result<Vec<(String, PatchFile)>, RepoError> {
        if !patch_dir.exists() {
            return Ok(Vec::new());
        }

        let mut patches = Vec::new();
        collect_patches(patch_dir, patch_dir, &mut patches)?;
        patches.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(patches)
    }

    /// パッチを削除する。`relative_path` は patches/ からの相対パス。
    pub fn delete(&self, relative_path: &str) -> Result<(), RepoError> {
        let path = self
            .patches_dir()
            .join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// 特定のパッチを読み込む。`relative_path` は patches/ からの相対パス。
    pub fn load(&self, relative_path: &str) -> Result<PatchFile, RepoError> {
        let path = self
            .patches_dir()
            .join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let content = fs::read(&path)?;
        let text = String::from_utf8(content)
            .map_err(|e| RepoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        let patch: PatchFile = serde_json::from_str(&text)?;
        if !SUPPORTED_VERSIONS.contains(&patch.version.as_str()) {
            return Err(RepoError::UnsupportedVersion(patch.version));
        }
        Ok(patch)
    }
}

fn normalize_path_separators(path: &str) -> String {
    path.replace('\\', "/")
}

fn collect_patches(
    root: &Path,
    current: &Path,
    out: &mut Vec<(String, PatchFile)>,
) -> Result<(), RepoError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_patches(root, &path, out)?;
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("dpatch") {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(|_| RepoError::InvalidPath(path.display().to_string()))?;
        let relative_str = relative
            .to_str()
            .ok_or_else(|| RepoError::InvalidPath(path.display().to_string()))?
            .replace('\\', "/");

        if let Ok(content) = fs::read(&path) {
            if let Ok(text) = String::from_utf8(content) {
                if let Ok(patch) = serde_json::from_str::<PatchFile>(&text) {
                    // 未対応バージョンは読み飛ばす（誤って no-op 適用しないため）
                    if SUPPORTED_VERSIONS.contains(&patch.version.as_str()) {
                        out.push((relative_str, patch));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::model::{PatchFile, PatchKind};

    fn dummy_patch(target_file: &str) -> PatchFile {
        PatchFile {
            version: "1".to_string(),
            id: "20260628-test-abc12345".to_string(),
            author: "test".to_string(),
            created_at: "2026-06-28T10:00:00+0900".to_string(),
            description: "テスト".to_string(),
            target_file: target_file.to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
            hunks: vec![],
        }
    }

    #[test]
    fn test_save_and_list_tree() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let patch = dummy_patch("src/Foo.java");

        repo.save(&patch, "20260628-test-abc12345.dpatch").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "src/Foo.java/20260628-test-abc12345.dpatch");
        assert_eq!(list[0].1.id, patch.id);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_multiple_patches_same_target() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let patch = dummy_patch("src/main/Foo.java");

        repo.save(&patch, "20260628-first.dpatch").unwrap();
        repo.save(&patch, "20260628-second.dpatch").unwrap();

        let list = repo.list().unwrap();
        assert_eq!(list.len(), 2);

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_list_from_dir() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let patch = dummy_patch("src/Bar.java");
        repo.save(&patch, "20260628-bar.dpatch").unwrap();

        let list = PatchRepository::list_from_dir(&repo.patches_dir()).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].0.contains("Bar.java"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_delete_tree_patch() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);
        let patch = dummy_patch("src/Del.java");
        repo.save(&patch, "20260628-del.dpatch").unwrap();

        repo.delete("src/Del.java/20260628-del.dpatch").unwrap();
        assert!(repo.list().unwrap().is_empty());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_unsupported_version_is_skipped_and_load_fails() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let patches_dir = tmp.join("patches");
        fs::create_dir_all(&patches_dir).unwrap();

        let mut patch = dummy_patch("Future.java");
        patch.version = "99".to_string();
        let json = serde_json::to_string_pretty(&patch).unwrap();
        fs::write(patches_dir.join("future.dpatch"), json).unwrap();

        let repo = PatchRepository::new(&tmp);
        // 一覧では読み飛ばされること
        assert!(repo.list().unwrap().is_empty());
        // 直接ロードでは明示的なエラーになること
        assert!(matches!(
            repo.load("future.dpatch"),
            Err(RepoError::UnsupportedVersion(_))
        ));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_save_rejects_invalid_patch() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let repo = PatchRepository::new(&tmp);

        // verify_tokens のない削除パッチは保存できないこと
        let mut patch = dummy_patch("src/Del.java");
        patch.kind = PatchKind::Delete;
        assert!(matches!(
            repo.save(&patch, "invalid.dpatch"),
            Err(RepoError::InvalidPatch(_))
        ));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_legacy_flat_patch_list() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_test_{}", uuid::Uuid::new_v4()));
        let patches_dir = tmp.join("patches");
        fs::create_dir_all(&patches_dir).unwrap();

        let patch = dummy_patch("Legacy.java");
        let json = serde_json::to_string_pretty(&patch).unwrap();
        fs::write(patches_dir.join("legacy.dpatch"), json).unwrap();

        let repo = PatchRepository::new(&tmp);
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "legacy.dpatch");

        let _ = fs::remove_dir_all(&tmp);
    }
}

use std::fs;
use std::path::PathBuf;

use crate::patch::model::PatchFile;

#[derive(Debug)]
pub enum RepoError {
    Io(std::io::Error),
    Json(serde_json::Error),
    #[allow(dead_code)]
    InvalidPath(String),
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepoError::Io(e) => write!(f, "I/Oエラー: {}", e),
            RepoError::Json(e) => write!(f, "JSONエラー: {}", e),
            RepoError::InvalidPath(p) => write!(f, "パスが無効: {}", p),
        }
    }
}

impl From<std::io::Error> for RepoError {
    fn from(e: std::io::Error) -> Self { RepoError::Io(e) }
}

impl From<serde_json::Error> for RepoError {
    fn from(e: serde_json::Error) -> Self { RepoError::Json(e) }
}

/// パッチリポジトリの読み書きを担当する。
/// ツール自体は git コマンドを実行せず、ファイルの読み書きのみを行う。
pub struct PatchRepository {
    pub repo_path: PathBuf,
}

impl PatchRepository {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self { repo_path: repo_path.into() }
    }

    /// patches/ ディレクトリのパスを返す
    pub fn patches_dir(&self) -> PathBuf {
        self.repo_path.join("patches")
    }

    /// パッチを保存する。patches/ ディレクトリを自動作成する。
    pub fn save(&self, patch: &PatchFile, filename: &str) -> Result<PathBuf, RepoError> {
        let dir = self.patches_dir();
        fs::create_dir_all(&dir)?;

        let path = dir.join(filename);
        let json = serde_json::to_string_pretty(patch)?;
        fs::write(&path, json.as_bytes())?;
        Ok(path)
    }

    /// patches/ ディレクトリのパッチ一覧を読み込む。
    /// 読み込み失敗したファイルはスキップする。
    pub fn list(&self) -> Result<Vec<(String, PatchFile)>, RepoError> {
        let dir = self.patches_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut patches = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("dpatch") {
                continue;
            }
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            if let Ok(content) = fs::read(&path) {
                if let Ok(text) = String::from_utf8(content) {
                    if let Ok(patch) = serde_json::from_str::<PatchFile>(&text) {
                        patches.push((filename, patch));
                    }
                }
            }
        }

        // ファイル名（タイムスタンププレフィックス）でソート
        patches.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(patches)
    }

    /// パッチファイルを削除する
    pub fn delete(&self, filename: &str) -> Result<(), RepoError> {
        let path = self.patches_dir().join(filename);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// 特定のファイルを読み込む
    #[allow(dead_code)]
    pub fn load(&self, filename: &str) -> Result<PatchFile, RepoError> {
        let path = self.patches_dir().join(filename);
        let content = fs::read(&path)?;
        let text = String::from_utf8(content)
            .map_err(|e| RepoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        let patch = serde_json::from_str(&text)?;
        Ok(patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::model::PatchFile;

    fn dummy_patch() -> PatchFile {
        PatchFile {
            version: "1".to_string(),
            id: "20260628-test-abc12345".to_string(),
            author: "test".to_string(),
            created_at: "2026-06-28T10:00:00+0900".to_string(),
            description: "テスト".to_string(),
            target_file: "Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            hunks: vec![],
        }
    }

    #[test]
    fn test_save_and_list() {
        let tmp = std::env::temp_dir().join("driftpatch_test");
        let repo = PatchRepository::new(&tmp);
        let patch = dummy_patch();

        repo.save(&patch, "20260628-test-abc12345.dpatch").unwrap();
        let list = repo.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].1.id, patch.id);

        // クリーンアップ
        let _ = fs::remove_dir_all(&tmp);
    }
}

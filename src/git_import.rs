use std::path::{Path, PathBuf};

use git2::{Diff, ObjectType, Repository, Sort};

use crate::encoding::decode_bytes;
use crate::lexer::profiles::detect_profile;
use crate::patch::context::ContextConfig;
use crate::patch::generator::{generate_patch, GeneratorError};
use crate::patch::model::{DiffHunk, PatchFile};

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub summary: String,
    pub time: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: String,
    pub status: FileChangeStatus,
}

#[derive(Debug, Clone)]
pub struct GeneratedPatch {
    pub target_file: String,
    pub patch: PatchFile,
    pub filename: String,
}

#[derive(Debug, Clone)]
pub struct SkippedEntry {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct PatchImportResult {
    pub generated: Vec<GeneratedPatch>,
    pub skipped: Vec<SkippedEntry>,
}

#[derive(Debug)]
pub enum GitError {
    NotARepo,
    InvalidCommit(String),
    Git2(git2::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::NotARepo => write!(f, "Git リポジトリではありません"),
            GitError::InvalidCommit(c) => write!(f, "無効なコミット: {}", c),
            GitError::Git2(e) => write!(f, "Git エラー: {}", e),
            GitError::Io(e) => write!(f, "I/O エラー: {}", e),
        }
    }
}

impl From<git2::Error> for GitError {
    fn from(e: git2::Error) -> Self {
        GitError::Git2(e)
    }
}

impl From<std::io::Error> for GitError {
    fn from(e: std::io::Error) -> Self {
        GitError::Io(e)
    }
}

/// コミット履歴を新しい順に列挙する。
pub fn list_commits(repo_path: &Path, limit: usize) -> Result<Vec<CommitInfo>, GitError> {
    let repo = open_repo(repo_path)?;
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TIME)?;
    revwalk.push_head()?;

    let mut commits = Vec::new();
    for oid in revwalk {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        commits.push(commit_to_info(&commit));
        if commits.len() >= limit {
            break;
        }
    }
    Ok(commits)
}

/// 指定コミットで変更されたファイル一覧を返す。
pub fn changed_files(repo_path: &Path, commit: &str) -> Result<Vec<ChangedFile>, GitError> {
    let repo = open_repo(repo_path)?;
    let commit = resolve_commit(&repo, commit)?;
    let diff = commit_diff(&repo, &commit)?;

    let mut files = Vec::new();
    diff.foreach(
        &mut |delta, _| {
            if let Some(entry) = delta_to_changed_file(&delta) {
                files.push(entry);
            }
            true
        },
        None,
        None,
        None,
    )?;
    Ok(files)
}

/// 指定コミットの全変更ファイルから .dpatch を生成する（ハンク単位で分割）。
pub fn generate_patches_from_commit(
    repo_path: &Path,
    commit: &str,
    work_dir: &Path,
    author: &str,
    description_override: Option<&str>,
    config: &ContextConfig,
) -> Result<PatchImportResult, GitError> {
    let repo = open_repo(repo_path)?;
    let commit_obj = resolve_commit(&repo, commit)?;
    let description = description_override
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| commit_summary(&commit_obj));

    let work_dir = canonicalize_or_self(work_dir)?;
    let diff = commit_diff(&repo, &commit_obj)?;

    let parent_tree = if commit_obj.parent_count() > 0 {
        Some(commit_obj.parent(0)?.tree()?)
    } else {
        None
    };
    let commit_tree = commit_obj.tree()?;

    let mut generated = Vec::new();
    let mut skipped = Vec::new();

    diff.foreach(
        &mut |delta, _| {
            if delta.flags().contains(git2::DiffFlags::BINARY) {
                if let Some(path) = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .and_then(|p| p.to_str())
                {
                    skipped.push(SkippedEntry {
                        path: path.replace('\\', "/"),
                        reason: "バイナリファイルはスキップ".to_string(),
                    });
                }
                return true;
            }

            let Some(changed) = delta_to_changed_file(&delta) else {
                return true;
            };

            if changed.status == FileChangeStatus::Deleted {
                skipped.push(SkippedEntry {
                    path: changed.path.clone(),
                    reason: "削除されたファイルはスキップ".to_string(),
                });
                return true;
            }

            let git_path = changed.path.replace('\\', "/");
            let target_file = match to_work_dir_relative(&work_dir, &git_path) {
                Ok(p) => p,
                Err(reason) => {
                    skipped.push(SkippedEntry {
                        path: git_path.clone(),
                        reason,
                    });
                    return true;
                }
            };

            let before_bytes = if let Some(ref parent) = parent_tree {
                read_blob_from_tree(&repo, parent, &git_path).unwrap_or(None)
            } else {
                None
            };
            let after_bytes =
                read_blob_from_tree(&repo, &commit_tree, &git_path).unwrap_or(None);

            let Some(after_bytes) = after_bytes else {
                skipped.push(SkippedEntry {
                    path: git_path.clone(),
                    reason: "コミット後のファイル内容が取得できません".to_string(),
                });
                return true;
            };

            if is_binary_content(before_bytes.as_deref())
                || is_binary_content(Some(&after_bytes))
            {
                skipped.push(SkippedEntry {
                    path: git_path.clone(),
                    reason: "バイナリファイルはスキップ".to_string(),
                });
                return true;
            }

            let (before_text, _) = decode_bytes(before_bytes.as_deref().unwrap_or(&[]));
            let (after_text, encoding) = decode_bytes(&after_bytes);

            let profile = detect_profile(Path::new(&target_file));
            match generate_patch(
                &before_text,
                &after_text,
                profile,
                author,
                &description,
                &target_file,
                &encoding,
                config,
            ) {
                Ok(patch) => {
                    for item in split_patch_by_hunks(patch) {
                        generated.push(GeneratedPatch {
                            target_file: target_file.clone(),
                            patch: item.patch,
                            filename: item.filename,
                        });
                    }
                }
                Err(GeneratorError::NoDiff) => {
                    skipped.push(SkippedEntry {
                        path: git_path,
                        reason: "変更が見つかりませんでした".to_string(),
                    });
                }
                Err(GeneratorError::NoMatch { hunk_index }) => {
                    skipped.push(SkippedEntry {
                        path: git_path,
                        reason: format!(
                            "ハンク {} の適用箇所が見つかりませんでした",
                            hunk_index
                        ),
                    });
                }
            }

            true
        },
        None,
        None,
        None,
    )?;

    Ok(PatchImportResult { generated, skipped })
}

struct SplitPatch {
    patch: PatchFile,
    filename: String,
}

/// PatchFile をハンク単位に分割する。複数ハンクの場合は id/filename に -h{N} を付与。
fn split_patch_by_hunks(mut patch: PatchFile) -> Vec<SplitPatch> {
    let base_id = patch.id.clone();
    let hunks: Vec<DiffHunk> = std::mem::take(&mut patch.hunks);

    if hunks.len() <= 1 {
        let filename = format!("{}.dpatch", patch.id);
        patch.hunks = hunks;
        return vec![SplitPatch { patch, filename }];
    }

    hunks.into_iter()
        .enumerate()
        .map(|(i, hunk)| {
            let suffix = format!("-h{}", i + 1);
            let id = format!("{}{}", base_id, suffix);
            let filename = format!("{}.dpatch", id);
            let single = PatchFile {
                id: id.clone(),
                hunks: vec![hunk],
                ..patch.clone()
            };
            SplitPatch {
                patch: single,
                filename,
            }
        })
        .collect()
}

fn open_repo(repo_path: &Path) -> Result<Repository, GitError> {
    Repository::open(repo_path).map_err(|e| {
        if e.code() == git2::ErrorCode::NotFound {
            GitError::NotARepo
        } else {
            GitError::Git2(e)
        }
    })
}

fn resolve_commit<'repo>(
    repo: &'repo Repository,
    commit: &str,
) -> Result<git2::Commit<'repo>, GitError> {
    repo.revparse_single(commit)
        .map_err(|_| GitError::InvalidCommit(commit.to_string()))?
        .peel_to_commit()
        .map_err(|_| GitError::InvalidCommit(commit.to_string()))
}

fn commit_diff<'repo>(
    repo: &'repo Repository,
    commit: &git2::Commit,
) -> Result<Diff<'repo>, GitError> {
    let commit_tree = commit.tree()?;
    if commit.parent_count() > 0 {
        let parent_tree = commit.parent(0)?.tree()?;
        Ok(repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?)
    } else {
        Ok(repo.diff_tree_to_tree(None, Some(&commit_tree), None)?)
    }
}

fn commit_to_info(commit: &git2::Commit) -> CommitInfo {
    let sha = commit.id().to_string();
    let short_sha = sha.chars().take(7).collect();
    let author = commit.author().name().unwrap_or("unknown").to_string();
    let summary = commit_summary(commit);
    let time = format_commit_time(commit.time());
    CommitInfo {
        sha,
        short_sha,
        author,
        summary,
        time,
    }
}

fn format_commit_time(time: git2::Time) -> String {
    use chrono::{FixedOffset, TimeZone};
    let offset_secs = time.offset_minutes() * 60;
    let offset = FixedOffset::east_opt(offset_secs).unwrap_or(FixedOffset::east_opt(0).unwrap());
    let dt = offset
        .timestamp_opt(time.seconds(), 0)
        .single()
        .unwrap_or_else(|| offset.timestamp_opt(0, 0).unwrap());
    dt.format("%Y-%m-%dT%H:%M:%S%z").to_string()
}

fn commit_summary(commit: &git2::Commit) -> String {
    commit
        .summary()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string()
}

fn delta_to_changed_file(delta: &git2::DiffDelta) -> Option<ChangedFile> {
    let status = match delta.status() {
        git2::Delta::Added => FileChangeStatus::Added,
        git2::Delta::Modified => FileChangeStatus::Modified,
        git2::Delta::Deleted => FileChangeStatus::Deleted,
        git2::Delta::Renamed => FileChangeStatus::Renamed,
        _ => return None,
    };

    let path = delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .and_then(|p| p.to_str())
        .map(|s| s.replace('\\', "/"))?;

    Some(ChangedFile { path, status })
}

fn read_blob_from_tree(
    repo: &Repository,
    tree: &git2::Tree,
    path: &str,
) -> Result<Option<Vec<u8>>, git2::Error> {
    match tree.get_path(Path::new(path)) {
        Ok(entry) => {
            if entry.kind() != Some(ObjectType::Blob) {
                return Ok(None);
            }
            let object = entry.to_object(repo)?;
            let blob = object.as_blob().expect("blob object");
            Ok(Some(blob.content().to_vec()))
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn is_binary_content(bytes: Option<&[u8]>) -> bool {
    bytes
        .map(|b| b.contains(&0))
        .unwrap_or(false)
}

fn canonicalize_or_self(path: &Path) -> Result<PathBuf, GitError> {
    std::fs::canonicalize(path).or_else(|_| Ok(path.to_path_buf()))
}

/// Git リポジトリルート相対パスを work_dir 相対パス（/ 区切り）に変換する。
fn to_work_dir_relative(work_dir: &Path, git_path: &str) -> Result<String, String> {
    let file_path = work_dir.join(git_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let file_canon = std::fs::canonicalize(&file_path).map_err(|_| {
        format!(
            "work_dir 配下にファイルがありません: {} (work_dir: {})",
            git_path,
            work_dir.display()
        )
    })?;

    let work_canon = canonicalize_or_self(work_dir).map_err(|e| e.to_string())?;
    let rel = file_canon
        .strip_prefix(&work_canon)
        .map_err(|_| format!("対象ファイルが work_dir 配下にありません: {}", git_path))?;

    Ok(rel
        .to_str()
        .unwrap_or("")
        .replace('\\', "/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch::context::ContextConfig;
    use git2::{IndexAddOption, Signature};
    use std::fs;

    fn init_repo_with_commits() -> (PathBuf, String) {
        let tmp = std::env::temp_dir().join(format!("driftpatch_git_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();

        // 初回コミット
        let file1 = tmp.join("src").join("Foo.java");
        fs::create_dir_all(file1.parent().unwrap()).unwrap();
        fs::write(
            &file1,
            "class Foo {\n    void a() {}\n    void b() {}\n}\n",
        )
        .unwrap();
        let file2 = tmp.join("src").join("Bar.java");
        fs::write(&file2, "class Bar {}\n").unwrap();

        commit_all(&repo, "initial");

        // 2箇所修正 + 新規ファイル
        fs::write(
            &file1,
            "class Foo {\n    void a() { System.out.println(1); }\n    void b() { System.out.println(2); }\n}\n",
        )
        .unwrap();
        let file3 = tmp.join("src").join("Baz.java");
        fs::write(&file3, "class Baz {}\n").unwrap();

        let oid = commit_all(&repo, "two changes and add");
        (tmp, oid.to_string())
    }

    fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = Signature::now("tester", "test@example.com").unwrap();
        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let oid = if let Some(parent) = parent {
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                message,
                &tree,
                &[&parent],
            )
            .unwrap()
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .unwrap()
        };
        oid
    }

    #[test]
    fn test_split_patch_by_hunks_single() {
        let patch = PatchFile {
            version: "1".to_string(),
            id: "20260628-test-abc12345".to_string(),
            author: "test".to_string(),
            created_at: "2026-06-28T10:00:00+0900".to_string(),
            description: "test".to_string(),
            target_file: "src/Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            hunks: vec![DiffHunk {
                context_before: vec![],
                removed: vec![],
                added_text: "x".to_string(),
                context_after: vec![],
                count: 1,
            }],
        };
        let split = split_patch_by_hunks(patch);
        assert_eq!(split.len(), 1);
        assert_eq!(split[0].filename, "20260628-test-abc12345.dpatch");
        assert_eq!(split[0].patch.hunks.len(), 1);
    }

    #[test]
    fn test_split_patch_by_hunks_multiple() {
        let patch = PatchFile {
            version: "1".to_string(),
            id: "20260628-test-abc12345".to_string(),
            author: "test".to_string(),
            created_at: "2026-06-28T10:00:00+0900".to_string(),
            description: "test".to_string(),
            target_file: "src/Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            hunks: vec![
                DiffHunk {
                    context_before: vec![],
                    removed: vec![],
                    added_text: "a".to_string(),
                    context_after: vec![],
                    count: 1,
                },
                DiffHunk {
                    context_before: vec![],
                    removed: vec![],
                    added_text: "b".to_string(),
                    context_after: vec![],
                    count: 1,
                },
            ],
        };
        let split = split_patch_by_hunks(patch);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].filename, "20260628-test-abc12345-h1.dpatch");
        assert_eq!(split[1].filename, "20260628-test-abc12345-h2.dpatch");
        assert_eq!(split[0].patch.hunks.len(), 1);
        assert_eq!(split[1].patch.hunks.len(), 1);
    }

    #[test]
    fn test_generate_patches_from_commit_splits_hunks() {
        let (repo_path, commit_sha) = init_repo_with_commits();
        let config = ContextConfig::default();

        let result = generate_patches_from_commit(
            &repo_path,
            &commit_sha,
            &repo_path,
            "tester",
            Some("import test"),
            &config,
        )
        .unwrap();

        let foo_patches: Vec<_> = result
            .generated
            .iter()
            .filter(|p| p.target_file == "src/Foo.java")
            .collect();
        assert!(
            foo_patches.len() >= 2,
            "expected at least 2 hunks for Foo.java, got {}",
            foo_patches.len()
        );
        for p in &foo_patches {
            assert_eq!(p.patch.hunks.len(), 1);
            assert!(p.filename.contains("-h"));
        }

        let baz_patches: Vec<_> = result
            .generated
            .iter()
            .filter(|p| p.target_file == "src/Baz.java")
            .collect();
        assert_eq!(baz_patches.len(), 1);
        assert_eq!(baz_patches[0].patch.hunks.len(), 1);

        let _ = fs::remove_dir_all(&repo_path);
    }

    #[test]
    fn test_list_commits() {
        let (repo_path, _) = init_repo_with_commits();
        let commits = list_commits(&repo_path, 10).unwrap();
        assert!(commits.len() >= 2);
        let _ = fs::remove_dir_all(&repo_path);
    }
}

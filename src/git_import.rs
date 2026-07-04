use std::path::{Path, PathBuf};

use git2::{Diff, ObjectType, Repository, Sort};

use crate::encoding::decode_bytes;
use crate::lexer::profiles::detect_profile;
use crate::patch::context::ContextConfig;
use crate::patch::generator::{generate_patch, GeneratorError};
use crate::patch::model::{DiffHunk, PatchFile, PatchKind, PATCH_FORMAT_VERSION};
use crate::patch::name_gen::generate_patch_id;
use crate::patch::verify::significant_token_texts;

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
    /// Renamed のみ: リネーム前の旧パス（リポジトリルート相対・`/` 区切り）
    pub old_path: Option<String>,
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

            let result = match changed.status {
                FileChangeStatus::Deleted => build_delete_patch(
                    &repo,
                    parent_tree.as_ref(),
                    &changed.path,
                    author,
                    &description,
                )
                .map(|item| vec![item]),
                FileChangeStatus::Renamed => match changed.old_path {
                    Some(ref old_git_path) => build_rename_patch(
                        &repo,
                        parent_tree.as_ref(),
                        &commit_tree,
                        old_git_path,
                        &changed.path,
                        author,
                        &description,
                        config,
                    )
                    .map(|item| vec![item]),
                    None => Err("リネーム元パスが取得できません".to_string()),
                },
                FileChangeStatus::Added | FileChangeStatus::Modified => build_content_patches(
                    &repo,
                    parent_tree.as_ref(),
                    &commit_tree,
                    &work_dir,
                    &changed,
                    author,
                    &description,
                    config,
                ),
            };

            match result {
                Ok(items) => generated.extend(items),
                Err(reason) => skipped.push(SkippedEntry {
                    path: changed.path.clone(),
                    reason,
                }),
            }

            true
        },
        None,
        None,
        None,
    )?;

    Ok(PatchImportResult { generated, skipped })
}

/// Added / Modified の変更内容から Modify / Create パッチ（複数可）を生成する。
fn build_content_patches(
    repo: &Repository,
    parent_tree: Option<&git2::Tree>,
    commit_tree: &git2::Tree,
    work_dir: &Path,
    changed: &ChangedFile,
    author: &str,
    description: &str,
    config: &ContextConfig,
) -> Result<Vec<GeneratedPatch>, String> {
    let git_path = changed.path.replace('\\', "/");
    // 新規作成は work_dir に現物が無い（pre-commit 状態の）場合もあるため字句的に解決する
    let target_file = if changed.status == FileChangeStatus::Added {
        to_work_dir_relative_lenient(&git_path)?
    } else {
        to_work_dir_relative(work_dir, &git_path)?
    };

    let before_bytes = if let Some(parent) = parent_tree {
        read_blob_from_tree(repo, parent, &git_path).unwrap_or(None)
    } else {
        None
    };
    let after_bytes = read_blob_from_tree(repo, commit_tree, &git_path)
        .unwrap_or(None)
        .ok_or_else(|| "コミット後のファイル内容が取得できません".to_string())?;

    if is_binary_content(before_bytes.as_deref()) || is_binary_content(Some(&after_bytes)) {
        return Err("バイナリファイルはスキップ".to_string());
    }

    let (before_text, _) = decode_bytes(before_bytes.as_deref().unwrap_or(&[]));
    let (after_text, encoding) = decode_bytes(&after_bytes);

    let profile = detect_profile(Path::new(&target_file));
    let mut patch = generate_patch(
        &before_text,
        &after_text,
        profile,
        author,
        description,
        &target_file,
        &encoding,
        config,
    )
    .map_err(generator_error_reason)?;

    if changed.status == FileChangeStatus::Added {
        patch.kind = PatchKind::Create;
    }

    // Create はファイル全文の 1 ハンクとして扱うため分割しない（分割は Modify のみ）
    let split = if patch.kind == PatchKind::Modify {
        split_patch_by_hunks(patch)
    } else {
        let filename = format!("{}.dpatch", patch.id);
        vec![SplitPatch { patch, filename }]
    };

    Ok(split
        .into_iter()
        .map(|item| GeneratedPatch {
            target_file: target_file.clone(),
            patch: item.patch,
            filename: item.filename,
        })
        .collect())
}

/// 削除ファイルから Delete パッチを生成する。
/// 適用時の誤削除を防ぐため、削除時点の significant token 列を verify_tokens に記録する。
fn build_delete_patch(
    repo: &Repository,
    parent_tree: Option<&git2::Tree>,
    git_path: &str,
    author: &str,
    description: &str,
) -> Result<GeneratedPatch, String> {
    let git_path = git_path.replace('\\', "/");
    let target_file = to_work_dir_relative_lenient(&git_path)?;

    let parent = parent_tree
        .ok_or_else(|| "親コミットがないため削除前の内容を取得できません".to_string())?;
    let before_bytes = read_blob_from_tree(repo, parent, &git_path)
        .unwrap_or(None)
        .ok_or_else(|| "削除前のファイル内容が取得できません".to_string())?;

    if is_binary_content(Some(&before_bytes)) {
        return Err("バイナリファイルはスキップ".to_string());
    }

    let (before_text, encoding) = decode_bytes(&before_bytes);
    let profile = detect_profile(Path::new(&target_file));
    let (id, created_at) = generate_patch_id(description);
    let filename = format!("{}.dpatch", id);

    let patch = PatchFile {
        version: PATCH_FORMAT_VERSION.to_string(),
        id,
        author: author.to_string(),
        created_at,
        description: description.to_string(),
        target_file: target_file.clone(),
        language: profile.name.to_string(),
        encoding,
        kind: PatchKind::Delete,
        old_path: None,
        verify_tokens: Some(significant_token_texts(&before_text, profile)),
        hunks: vec![],
    };

    Ok(GeneratedPatch {
        target_file,
        patch,
        filename,
    })
}

/// リネームから Rename パッチを生成する。
/// 内容が実質同一（significant token 一致）なら検証付き移動のみの純リネーム、
/// 差分があれば移動 + ハンク適用のパッチになる。
fn build_rename_patch(
    repo: &Repository,
    parent_tree: Option<&git2::Tree>,
    commit_tree: &git2::Tree,
    old_git_path: &str,
    new_git_path: &str,
    author: &str,
    description: &str,
    config: &ContextConfig,
) -> Result<GeneratedPatch, String> {
    let old_git_path = old_git_path.replace('\\', "/");
    let new_git_path = new_git_path.replace('\\', "/");
    let old_rel = to_work_dir_relative_lenient(&old_git_path)?;
    let target_file = to_work_dir_relative_lenient(&new_git_path)?;

    let parent = parent_tree
        .ok_or_else(|| "親コミットがないためリネーム元の内容を取得できません".to_string())?;
    let before_bytes = read_blob_from_tree(repo, parent, &old_git_path)
        .unwrap_or(None)
        .ok_or_else(|| "リネーム前のファイル内容が取得できません".to_string())?;
    let after_bytes = read_blob_from_tree(repo, commit_tree, &new_git_path)
        .unwrap_or(None)
        .ok_or_else(|| "リネーム後のファイル内容が取得できません".to_string())?;

    if is_binary_content(Some(&before_bytes)) || is_binary_content(Some(&after_bytes)) {
        return Err("バイナリファイルはスキップ".to_string());
    }

    let (before_text, _) = decode_bytes(&before_bytes);
    let (after_text, encoding) = decode_bytes(&after_bytes);
    let profile = detect_profile(Path::new(&target_file));

    let before_sig = significant_token_texts(&before_text, profile);
    let content_patch = if before_sig == significant_token_texts(&after_text, profile) {
        None
    } else {
        match generate_patch(
            &before_text,
            &after_text,
            profile,
            author,
            description,
            &target_file,
            &encoding,
            config,
        ) {
            Ok(p) => Some(p),
            // 空白のみの差は diff が出ないため純リネームとして扱う
            Err(GeneratorError::NoDiff) => None,
            Err(e) => return Err(generator_error_reason(e)),
        }
    };

    let mut patch = match content_patch {
        Some(p) => p,
        None => {
            let (id, created_at) = generate_patch_id(description);
            PatchFile {
                version: PATCH_FORMAT_VERSION.to_string(),
                id,
                author: author.to_string(),
                created_at,
                description: description.to_string(),
                target_file: target_file.clone(),
                language: profile.name.to_string(),
                encoding,
                kind: PatchKind::Rename,
                old_path: None,
                verify_tokens: Some(before_sig),
                hunks: vec![],
            }
        }
    };
    patch.kind = PatchKind::Rename;
    patch.old_path = Some(old_rel);

    // リネームはハンク分割しない（分割すると 2 個目以降の適用時に旧ファイルが既に無く破綻する）
    let filename = format!("{}.dpatch", patch.id);
    Ok(GeneratedPatch {
        target_file,
        patch,
        filename,
    })
}

fn generator_error_reason(e: GeneratorError) -> String {
    match e {
        GeneratorError::NoDiff => "変更が見つかりませんでした".to_string(),
        GeneratorError::NoMatch { hunk_index } => {
            format!("ハンク {} の適用箇所が見つかりませんでした", hunk_index)
        }
    }
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
    let mut diff = if commit.parent_count() > 0 {
        let parent_tree = commit.parent(0)?.tree()?;
        repo.diff_tree_to_tree(Some(&parent_tree), Some(&commit_tree), None)?
    } else {
        repo.diff_tree_to_tree(None, Some(&commit_tree), None)?
    };

    // リネームを Delete+Add の 2 デルタではなく 1 つの Renamed デルタとして検出する
    let mut find_opts = git2::DiffFindOptions::new();
    find_opts.renames(true);
    diff.find_similar(Some(&mut find_opts))?;

    Ok(diff)
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

    let old_path = if status == FileChangeStatus::Renamed {
        delta
            .old_file()
            .path()
            .and_then(|p| p.to_str())
            .map(|s| s.replace('\\', "/"))
    } else {
        None
    };

    Some(ChangedFile {
        path,
        status,
        old_path,
    })
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

/// 実在チェックなしの work_dir 相対パス解決。
/// 削除・新規作成・リネームでは work_dir に現物が存在しない場合があるため字句的に解決する。
/// `..` セグメントは work_dir 外への破壊的操作（削除・書込）を防ぐため拒否する。
fn to_work_dir_relative_lenient(git_path: &str) -> Result<String, String> {
    let normalized = git_path.replace('\\', "/");
    if normalized.is_empty() {
        return Err("パスが空です".to_string());
    }
    if normalized.split('/').any(|seg| seg == "..") {
        return Err(format!("不正なパス（work_dir 外参照）: {}", git_path));
    }
    Ok(normalized)
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
        // add_all はワーキングツリーから消えたファイルを index から除去しないため、
        // 削除・リネームを反映するには update_all も必要
        index.update_all(["*"].iter(), None).unwrap();
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
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
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
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
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
        // 新規追加ファイルは Create パッチになること
        assert_eq!(baz_patches[0].patch.kind, PatchKind::Create);

        let _ = fs::remove_dir_all(&repo_path);
    }

    #[test]
    fn test_generate_delete_patch_from_commit() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_git_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();

        let content = "class Legacy {\n    void a() {}\n}\n";
        let file = tmp.join("src").join("Legacy.java");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, content).unwrap();
        commit_all(&repo, "initial");

        fs::remove_file(&file).unwrap();
        let oid = commit_all(&repo, "remove legacy");

        let result = generate_patches_from_commit(
            &tmp,
            &oid.to_string(),
            &tmp,
            "tester",
            Some("delete test"),
            &ContextConfig::default(),
        )
        .unwrap();

        // スキップではなく Delete パッチとして生成されること
        assert!(result.skipped.is_empty(), "skipped: {:?}", result.skipped);
        assert_eq!(result.generated.len(), 1);
        let item = &result.generated[0];
        assert_eq!(item.target_file, "src/Legacy.java");
        assert_eq!(item.patch.kind, PatchKind::Delete);
        assert!(item.patch.hunks.is_empty());
        let tokens = item.patch.verify_tokens.as_ref().unwrap();
        assert!(tokens.iter().any(|t| t == "Legacy"));
        assert!(item.patch.validate().is_ok());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_rename_detected_with_find_similar() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_git_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();

        let content = "class Moved {\n    void a() {}\n    void b() {}\n}\n";
        let old_file = tmp.join("src").join("OldName.java");
        fs::create_dir_all(old_file.parent().unwrap()).unwrap();
        fs::write(&old_file, content).unwrap();
        commit_all(&repo, "initial");

        // 内容変更なしの純リネーム
        fs::rename(&old_file, tmp.join("src").join("NewName.java")).unwrap();
        let oid = commit_all(&repo, "rename");

        let result = generate_patches_from_commit(
            &tmp,
            &oid.to_string(),
            &tmp,
            "tester",
            Some("rename test"),
            &ContextConfig::default(),
        )
        .unwrap();

        // Delete+Create の 2 パッチではなく 1 つの Rename パッチになること
        assert_eq!(result.generated.len(), 1, "generated: {:?}", result.generated);
        let item = &result.generated[0];
        assert_eq!(item.patch.kind, PatchKind::Rename);
        assert_eq!(item.target_file, "src/NewName.java");
        assert_eq!(item.patch.old_path.as_deref(), Some("src/OldName.java"));
        assert!(item.patch.hunks.is_empty(), "純リネームはハンクなし");
        assert!(item.patch.verify_tokens.is_some());
        assert!(item.patch.validate().is_ok());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_rename_with_edit_generates_hunks_not_split() {
        let tmp = std::env::temp_dir().join(format!("driftpatch_git_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&tmp).unwrap();
        let repo = Repository::init(&tmp).unwrap();

        let before = "class Refactored {\n    void a() {}\n    void b() {}\n    void c() {}\n}\n";
        let old_file = tmp.join("src").join("BeforeEdit.java");
        fs::create_dir_all(old_file.parent().unwrap()).unwrap();
        fs::write(&old_file, before).unwrap();
        commit_all(&repo, "initial");

        // リネーム + 軽微な編集（類似度は保たれる）
        fs::remove_file(&old_file).unwrap();
        let after = "class Refactored {\n    void a() { System.out.println(1); }\n    void b() {}\n    void c() {}\n}\n";
        fs::write(tmp.join("src").join("AfterEdit.java"), after).unwrap();
        let oid = commit_all(&repo, "rename with edit");

        let result = generate_patches_from_commit(
            &tmp,
            &oid.to_string(),
            &tmp,
            "tester",
            Some("rename edit test"),
            &ContextConfig::default(),
        )
        .unwrap();

        assert_eq!(result.generated.len(), 1, "generated: {:?}", result.generated);
        let item = &result.generated[0];
        assert_eq!(item.patch.kind, PatchKind::Rename);
        assert_eq!(item.patch.old_path.as_deref(), Some("src/BeforeEdit.java"));
        assert!(!item.patch.hunks.is_empty(), "編集ありリネームはハンクを持つこと");
        // -h 分割されないこと
        assert!(!item.filename.contains("-h"), "filename: {}", item.filename);
        assert!(item.patch.validate().is_ok());

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn test_list_commits() {
        let (repo_path, _) = init_repo_with_commits();
        let commits = list_commits(&repo_path, 10).unwrap();
        assert!(commits.len() >= 2);
        let _ = fs::remove_dir_all(&repo_path);
    }
}

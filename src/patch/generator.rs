use crate::diff::token_diff::{diff_tokens, DiffTag, extract_significant};
use crate::lexer::{GenericTokenizer, LanguageProfile, Token};
use crate::patch::context::{find_patch_matches, ContextConfig, CONTEXT_STEPS};
use crate::patch::model::{DiffHunk, PatchFile};
use crate::patch::name_gen::generate_patch_id;

#[derive(Debug)]
pub enum GeneratorError {
    NoDiff,
    /// パッチ生成時に一意性が取れなかった（最大コンテキストでもマッチ複数）
    NotUnique {
        hunk_index: usize,
        match_count: usize,
    },
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneratorError::NoDiff => write!(f, "変更が見つかりませんでした"),
            GeneratorError::NotUnique { hunk_index, match_count } => write!(
                f,
                "ハンク {} のコンテキストが一意に特定できません（{} 箇所マッチ）。手動確認が必要です。",
                hunk_index, match_count
            ),
        }
    }
}

/// 生のハンク情報（diff 解析後の中間表現）
struct RawHunk {
    /// orig_sig 内での変更開始位置（挿入のみの場合は挿入点）
    orig_start: usize,
    /// orig_sig 内での変更終了位置（exclusive）
    orig_end: usize,
    /// orig_sig から削除されるトークンのインデックス
    removed: Vec<usize>,
    /// edit_sig から追加されるトークンのインデックス
    added: Vec<usize>,
}

/// 元テキストと編集テキストからパッチファイルを生成する。
pub fn generate_patch(
    original: &str,
    edited: &str,
    profile: &'static LanguageProfile,
    author: &str,
    description: &str,
    target_file: &str,
    encoding: &str,
    config: &ContextConfig,
) -> Result<PatchFile, GeneratorError> {
    let tokenizer = GenericTokenizer::new(profile);
    let orig_tokens = tokenizer.tokenize(original);
    let edit_tokens = tokenizer.tokenize(edited);

    let ops = diff_tokens(&orig_tokens, &edit_tokens);

    let has_diff = ops.iter().any(|o| o.tag != DiffTag::Equal);
    if !has_diff {
        return Err(GeneratorError::NoDiff);
    }

    let orig_sig: Vec<&Token> = extract_significant(&orig_tokens);
    let edit_sig: Vec<&Token> = extract_significant(&edit_tokens);

    let hunks_raw = group_hunks(&ops, &orig_sig, &edit_sig);

    if hunks_raw.is_empty() {
        return Err(GeneratorError::NoDiff);
    }

    let mut hunks = Vec::new();
    for (hunk_idx, raw) in hunks_raw.iter().enumerate() {
        let removed: Vec<Token> = raw.removed.iter().map(|&i| (*orig_sig[i]).clone()).collect();
        let added: Vec<Token> = raw.added.iter().map(|&i| (*edit_sig[i]).clone()).collect();

        let change_start = raw.orig_start;
        let change_end = raw.orig_end;

        // コンテキストを段階的に拡張して一意性を確保
        let mut found_hunk: Option<DiffHunk> = None;
        for &ctx_size in CONTEXT_STEPS {
            if ctx_size > config.max_context {
                break;
            }

            let before_start = change_start.saturating_sub(ctx_size);
            let after_end = (change_end + ctx_size).min(orig_sig.len());

            let ctx_before: Vec<Token> = orig_sig[before_start..change_start]
                .iter()
                .map(|t| (*t).clone())
                .collect();
            let ctx_after: Vec<Token> = orig_sig[change_end..after_end]
                .iter()
                .map(|t| (*t).clone())
                .collect();

            if config.uniqueness_check {
                let matches = find_patch_matches(&orig_sig, &ctx_before, &removed, &ctx_after);
                if matches.len() == 1 {
                    found_hunk = Some(DiffHunk {
                        context_before: ctx_before,
                        removed: removed.clone(),
                        added: added.clone(),
                        context_after: ctx_after,
                    });
                    break;
                }
            } else {
                found_hunk = Some(DiffHunk {
                    context_before: ctx_before,
                    removed: removed.clone(),
                    added: added.clone(),
                    context_after: ctx_after,
                });
                break;
            }
        }

        if let Some(hunk) = found_hunk {
            hunks.push(hunk);
        } else {
            let ctx_size = config.max_context;
            let before_start = change_start.saturating_sub(ctx_size);
            let after_end = (change_end + ctx_size).min(orig_sig.len());
            let ctx_before: Vec<Token> = orig_sig[before_start..change_start]
                .iter()
                .map(|t| (*t).clone())
                .collect();
            let ctx_after: Vec<Token> = orig_sig[change_end..after_end]
                .iter()
                .map(|t| (*t).clone())
                .collect();
            let matches = find_patch_matches(&orig_sig, &ctx_before, &removed, &ctx_after);
            return Err(GeneratorError::NotUnique {
                hunk_index: hunk_idx,
                match_count: matches.len(),
            });
        }
    }

    let (id, created_at) = generate_patch_id(description);

    Ok(PatchFile {
        version: "1".to_string(),
        id,
        author: author.to_string(),
        created_at,
        description: description.to_string(),
        target_file: target_file.to_string(),
        language: profile.name.to_string(),
        encoding: encoding.to_string(),
        hunks,
    })
}

/// diff ops から連続する変更ブロック（ハンク）を抽出する。
/// 挿入のみのハンクでは、挿入点（orig 内での挿入位置）を orig_start = orig_end で表す。
fn group_hunks(
    ops: &[crate::diff::DiffOp],
    _orig_sig: &[&Token],
    _edit_sig: &[&Token],
) -> Vec<RawHunk> {
    let mut hunks = Vec::new();
    let mut current_removed: Vec<usize> = Vec::new();
    let mut current_added: Vec<usize> = Vec::new();
    // 直前の Equal op の次の orig_sig インデックス（挿入点の計算に使う）
    let mut next_orig_after_equal: usize = 0;

    for op in ops {
        match op.tag {
            DiffTag::Delete => {
                if let Some(idx) = op.a_index {
                    current_removed.push(idx);
                }
            }
            DiffTag::Insert => {
                if let Some(idx) = op.b_index {
                    current_added.push(idx);
                }
            }
            DiffTag::Equal => {
                // 変更ブロックが終わった: フラッシュ
                if !current_removed.is_empty() || !current_added.is_empty() {
                    let orig_start = current_removed.first().copied()
                        .unwrap_or(next_orig_after_equal);
                    let orig_end = current_removed.last().map(|&i| i + 1)
                        .unwrap_or(orig_start);
                    hunks.push(RawHunk {
                        orig_start,
                        orig_end,
                        removed: current_removed.clone(),
                        added: current_added.clone(),
                    });
                    current_removed.clear();
                    current_added.clear();
                }
                // 次の Equal の直後の orig インデックスを記録
                if let Some(idx) = op.a_index {
                    next_orig_after_equal = idx + 1;
                }
            }
        }
    }
    // 末尾の未フラッシュ分
    if !current_removed.is_empty() || !current_added.is_empty() {
        let orig_start = current_removed.first().copied()
            .unwrap_or(next_orig_after_equal);
        let orig_end = current_removed.last().map(|&i| i + 1)
            .unwrap_or(orig_start);
        hunks.push(RawHunk {
            orig_start,
            orig_end,
            removed: current_removed,
            added: current_added,
        });
    }

    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;
    use crate::patch::context::ContextConfig;

    #[test]
    fn test_generate_patch_basic() {
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();
        let result = generate_patch(orig, edit, &JAVA, "tester", "テスト", "Foo.java", "UTF-8", &config);
        assert!(result.is_ok(), "パッチ生成失敗: {:?}", result);
        let patch = result.unwrap();
        assert!(!patch.hunks.is_empty());
        assert_eq!(patch.author, "tester");
        // added トークンが含まれていることを確認
        let added_texts: Vec<&str> = patch.hunks[0].added.iter().map(|t| t.text.as_str()).collect();
        assert!(added_texts.contains(&"Objects"));
    }

    #[test]
    fn test_generate_patch_no_diff() {
        let orig = "int x = 1;";
        let config = ContextConfig::default();
        let result = generate_patch(orig, orig, &JAVA, "tester", "テスト", "Foo.java", "UTF-8", &config);
        assert!(matches!(result, Err(GeneratorError::NoDiff)));
    }
}

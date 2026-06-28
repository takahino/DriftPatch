use crate::diff::token_diff::{diff_tokens, DiffTag, extract_significant};
use crate::lexer::{GenericTokenizer, LanguageProfile, Token};
use crate::patch::context::{find_patch_matches, ContextConfig, CONTEXT_STEPS};
use crate::patch::model::{DiffHunk, PatchFile};
use crate::patch::name_gen::generate_patch_id;

#[derive(Debug)]
pub enum GeneratorError {
    NoDiff,
    /// パッチ生成時にマッチ箇所が見つからなかった
    NoMatch { hunk_index: usize },
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneratorError::NoDiff => write!(f, "変更が見つかりませんでした"),
            GeneratorError::NoMatch { hunk_index } => write!(
                f,
                "ハンク {} の適用箇所が見つかりませんでした",
                hunk_index
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
    let orig_to_edit_sig = build_orig_to_edit_sig_map(&ops);
    let edit_sig_to_token = sig_to_token_indices(&edit_tokens);

    let hunks_raw = group_hunks(&ops, &orig_sig, &edit_sig);

    if hunks_raw.is_empty() {
        return Err(GeneratorError::NoDiff);
    }

    let mut hunks = Vec::new();
    for (hunk_idx, raw) in hunks_raw.iter().enumerate() {
        let removed: Vec<Token> = raw.removed.iter().map(|&i| (*orig_sig[i]).clone()).collect();

        let change_start = raw.orig_start;
        let change_end = raw.orig_end;

        // コンテキストを段階的に拡張し、最小マッチ数を与えるコンテキストを選ぶ
        let mut found_hunk: Option<DiffHunk> = None;
        let mut best_match_count = usize::MAX;

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

            let matches =
                find_patch_matches(&orig_sig, &ctx_before, &removed, &ctx_after);
            let match_count = matches.len();

            if match_count == 0 {
                continue;
            }

            if match_count < best_match_count {
                best_match_count = match_count;
                let added_text = extract_added_text(
                    edited,
                    &edit_tokens,
                    &orig_to_edit_sig,
                    &edit_sig_to_token,
                    change_start,
                    change_end,
                    !ctx_before.is_empty(),
                    !ctx_after.is_empty(),
                );
                found_hunk = Some(DiffHunk {
                    context_before: ctx_before,
                    removed: removed.clone(),
                    added_text,
                    context_after: ctx_after,
                    count: match_count,
                });
            }
        }

        if let Some(hunk) = found_hunk {
            hunks.push(hunk);
        } else {
            return Err(GeneratorError::NoMatch { hunk_index: hunk_idx });
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
    let mut current_has_insert = false;
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
                current_has_insert = true;
            }
            DiffTag::Equal => {
                // 変更ブロックが終わった: フラッシュ
                if !current_removed.is_empty() || current_has_insert {
                    let orig_start = current_removed.first().copied()
                        .unwrap_or(next_orig_after_equal);
                    let orig_end = current_removed.last().map(|&i| i + 1)
                        .unwrap_or(orig_start);
                    hunks.push(RawHunk {
                        orig_start,
                        orig_end,
                        removed: current_removed.clone(),
                    });
                    current_removed.clear();
                    current_has_insert = false;
                }
                // 次の Equal の直後の orig インデックスを記録
                if let Some(idx) = op.a_index {
                    next_orig_after_equal = idx + 1;
                }
            }
        }
    }
    // 末尾の未フラッシュ分
    if !current_removed.is_empty() || current_has_insert {
        let orig_start = current_removed.first().copied()
            .unwrap_or(next_orig_after_equal);
        let orig_end = current_removed.last().map(|&i| i + 1)
            .unwrap_or(orig_start);
        hunks.push(RawHunk {
            orig_start,
            orig_end,
            removed: current_removed,
        });
    }

    hunks
}

/// Equal op から orig_sig インデックス → edit_sig インデックスのマップを構築する。
fn build_orig_to_edit_sig_map(ops: &[crate::diff::DiffOp]) -> Vec<Option<usize>> {
    let max_a = ops
        .iter()
        .filter_map(|o| o.a_index)
        .max()
        .map(|m| m + 1)
        .unwrap_or(0);
    let mut map = vec![None; max_a];
    for op in ops {
        if op.tag == DiffTag::Equal {
            if let (Some(a), Some(b)) = (op.a_index, op.b_index) {
                map[a] = Some(b);
            }
        }
    }
    map
}

/// significant トークンインデックス → フルトークン列インデックス
fn sig_to_token_indices(tokens: &[Token]) -> Vec<usize> {
    tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_significant())
        .map(|(i, _)| i)
        .collect()
}

/// edited ソースからコンテキスト境界に挟まれた verbatim 置換文字列を抽出する。
fn extract_added_text(
    edited: &str,
    edit_tokens: &[Token],
    orig_to_edit_sig: &[Option<usize>],
    edit_sig_to_token: &[usize],
    change_start: usize,
    change_end: usize,
    has_ctx_before: bool,
    has_ctx_after: bool,
) -> String {
    let added_start_byte = if has_ctx_before && change_start > 0 {
        let last_ctx_before_orig = change_start - 1;
        let edit_sig_idx = orig_to_edit_sig[last_ctx_before_orig]
            .expect("context_before token must map to equal in edited");
        let edit_token_idx = edit_sig_to_token[edit_sig_idx];
        edit_tokens[edit_token_idx].byte_end()
    } else {
        0
    };

    let added_end_byte = if has_ctx_after && change_end < orig_to_edit_sig.len() {
        let first_ctx_after_orig = change_end;
        let edit_sig_idx = orig_to_edit_sig[first_ctx_after_orig]
            .expect("context_after token must map to equal in edited");
        let edit_token_idx = edit_sig_to_token[edit_sig_idx];
        edit_tokens[edit_token_idx].start
    } else {
        edited.len()
    };

    edited[added_start_byte..added_end_byte].to_string()
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
        // added_text に変更内容が含まれていることを確認
        assert!(patch.hunks[0].added_text.contains("Objects"));
    }

    #[test]
    fn test_generate_patch_no_diff() {
        let orig = "int x = 1;";
        let config = ContextConfig::default();
        let result = generate_patch(orig, orig, &JAVA, "tester", "テスト", "Foo.java", "UTF-8", &config);
        assert!(matches!(result, Err(GeneratorError::NoDiff)));
    }

    #[test]
    fn test_generate_patch_count_one_for_unique() {
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(orig, edit, &JAVA, "tester", "テスト", "Foo.java", "UTF-8", &config).unwrap();
        assert_eq!(patch.hunks[0].count, 1);
    }

    #[test]
    fn test_generate_patch_count_multiple_for_repeated() {
        // 同一パターンが2箇所あり、両方を同じ変更にした場合は count=2
        let orig = "void foo() { return null; } void bar() { return null; }";
        let edit = "void foo() { return 0; } void bar() { return 0; }";
        let config = ContextConfig::default();
        let patch = generate_patch(orig, edit, &JAVA, "tester", "テスト", "Foo.java", "UTF-8", &config).unwrap();
        // 2つのハンクに分かれるか、1ハンクで count=2 になるかは diff のグルーピング次第
        let total_count: usize = patch.hunks.iter().map(|h| h.count).sum();
        assert_eq!(total_count, 2, "2箇所の変更がカバーされること");
    }
}

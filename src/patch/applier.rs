use crate::lexer::{GenericTokenizer, LanguageProfile, Token};
use crate::patch::context::find_patch_matches;
use crate::patch::model::{DiffHunk, PatchFile};

#[derive(Debug)]
pub enum ApplyError {
    /// 対象箇所が見つからない
    NoMatch { hunk_index: usize },
    /// 期待マッチ数と実際のマッチ数が一致しない（ドリフト検出）
    CountMismatch {
        hunk_index: usize,
        expected: usize,
        actual: usize,
        positions: Vec<usize>,
    },
    /// 複数マッチの置換範囲が重なる
    OverlappingMatches { hunk_index: usize },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use crate::i18n::tr_args;
        match self {
            ApplyError::NoMatch { hunk_index } => {
                write!(
                    f,
                    "{}",
                    tr_args("apply.no_match", &[("hunk", &hunk_index.to_string())])
                )
            }
            ApplyError::CountMismatch {
                hunk_index,
                expected,
                actual,
                positions,
            } => {
                write!(
                    f,
                    "{}",
                    tr_args(
                        "apply.count_mismatch",
                        &[
                            ("hunk", &hunk_index.to_string()),
                            ("expected", &expected.to_string()),
                            ("actual", &actual.to_string()),
                            ("positions", &format!("{:?}", positions)),
                        ]
                    )
                )
            }
            ApplyError::OverlappingMatches { hunk_index } => {
                write!(
                    f,
                    "{}",
                    tr_args("apply.overlapping", &[("hunk", &hunk_index.to_string())])
                )
            }
        }
    }
}

/// パッチを適用してテキストを返す。
/// significant tokens でマッチ位置を特定し、added_text を verbatim で挿入する。
pub fn apply_patch(
    target_text: &str,
    patch: &PatchFile,
    profile: &'static LanguageProfile,
) -> Result<String, ApplyError> {
    let mut current = target_text.to_string();

    for (hunk_idx, hunk) in patch.hunks.iter().enumerate() {
        current = apply_hunk(&current, hunk_idx, hunk, profile)?;
    }

    Ok(current)
}

/// `apply_patch_lenient` の結果。`skipped_hunks` は「既に適用済み」と判定され
/// 実際には変更しなかったハンクのインデックス（`patch.hunks` 基準）。
#[derive(Debug, Clone)]
pub struct LenientApplyOutcome {
    pub text: String,
    pub skipped_hunks: Vec<usize>,
}

/// パッチを適用する。ただし各ハンクが `NoMatch` になった場合、対象テキストが
/// 既にそのハンクの適用結果と一致するか（冪等）を追加で判定し、一致するなら
/// そのハンクをスキップして処理を続行する（テキストは変更しない）。
///
/// 安全条件: 通常パターン（context_before + removed + context_after）のマッチが
/// 0 件、かつ「適用済みパターン」（context_before + added の significant tokens
/// + context_after）のマッチ数がハンクの期待マッチ数（`hunk.count`）と正確に
/// 一致する場合のみ「適用済み」とみなす。マッチ数が一致しない場合は通常どおり
/// `NoMatch` / `CountMismatch` を返し、部分適用（ドリフト）として扱う。
pub fn apply_patch_lenient(
    target_text: &str,
    patch: &PatchFile,
    profile: &'static LanguageProfile,
) -> Result<LenientApplyOutcome, ApplyError> {
    let mut current = target_text.to_string();
    let mut skipped_hunks = Vec::new();

    for (hunk_idx, hunk) in patch.hunks.iter().enumerate() {
        match apply_hunk(&current, hunk_idx, hunk, profile) {
            Ok(next) => current = next,
            Err(ApplyError::NoMatch { hunk_index }) => {
                if hunk_already_applied(&current, hunk, profile) {
                    skipped_hunks.push(hunk_idx);
                } else {
                    return Err(ApplyError::NoMatch { hunk_index });
                }
            }
            Err(e) => return Err(e),
        }
    }

    Ok(LenientApplyOutcome {
        text: current,
        skipped_hunks,
    })
}

/// `text` が既にこのハンクの適用結果と一致するか判定する。
/// `context_before + significant(added_text) + context_after` のパターンが
/// ハンクの期待マッチ数とちょうど一致する場合のみ true を返す。
fn hunk_already_applied(text: &str, hunk: &DiffHunk, profile: &'static LanguageProfile) -> bool {
    let tokenizer = GenericTokenizer::new(profile);
    let tokens = tokenizer.tokenize(text);
    let sig: Vec<&Token> = tokens.iter().filter(|t| t.is_significant()).collect();

    let added_sig: Vec<Token> = tokenizer
        .tokenize(&hunk.added_text)
        .into_iter()
        .filter(|t| t.is_significant())
        .collect();

    let matches = find_patch_matches(&sig, &hunk.context_before, &added_sig, &hunk.context_after);
    matches.len() == hunk.count
}

fn apply_hunk(
    text: &str,
    hunk_idx: usize,
    hunk: &DiffHunk,
    profile: &'static LanguageProfile,
) -> Result<String, ApplyError> {
    let tokenizer = GenericTokenizer::new(profile);
    let tokens = tokenizer.tokenize(text);
    let sig: Vec<&Token> = tokens.iter().filter(|t| t.is_significant()).collect();

    let matches = find_patch_matches(
        &sig,
        &hunk.context_before,
        &hunk.removed,
        &hunk.context_after,
    );

    if matches.is_empty() {
        return Err(ApplyError::NoMatch {
            hunk_index: hunk_idx,
        });
    }

    if matches.len() != hunk.count {
        return Err(ApplyError::CountMismatch {
            hunk_index: hunk_idx,
            expected: hunk.count,
            actual: matches.len(),
            positions: matches,
        });
    }

    let sig_to_token_idx: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_significant())
        .map(|(i, _)| i)
        .collect();

    let has_ctx_before = !hunk.context_before.is_empty();
    let has_ctx_after = !hunk.context_after.is_empty();

    let mut ranges: Vec<(usize, usize)> = matches
        .iter()
        .map(|&match_start_in_sig| {
            compute_change_byte_range(
                &tokens,
                &sig_to_token_idx,
                match_start_in_sig,
                hunk.removed.len(),
                has_ctx_before,
                has_ctx_after,
            )
        })
        .collect();

    // 重なりチェック
    ranges.sort_by_key(|&(start, _)| start);
    for window in ranges.windows(2) {
        if window[0].1 > window[1].0 {
            return Err(ApplyError::OverlappingMatches {
                hunk_index: hunk_idx,
            });
        }
    }

    // 末尾から順に置換（オフセットずれを回避）
    ranges.sort_by_key(|&(start, _)| std::cmp::Reverse(start));

    let mut result = text.to_string();
    for (change_start_byte, change_end_byte) in ranges {
        result = format!(
            "{}{}{}",
            &result[..change_start_byte],
            &hunk.added_text,
            &result[change_end_byte..]
        );
    }

    Ok(result)
}

/// significant トークン列上のマッチ位置から、target テキスト上の byte 置換範囲を計算する。
fn compute_change_byte_range(
    tokens: &[Token],
    sig_to_token_idx: &[usize],
    match_start_in_sig: usize,
    removed_len: usize,
    has_ctx_before: bool,
    has_ctx_after: bool,
) -> (usize, usize) {
    let change_start_byte = if has_ctx_before && match_start_in_sig > 0 {
        let last_ctx_before_sig = match_start_in_sig - 1;
        let token_idx = sig_to_token_idx[last_ctx_before_sig];
        tokens[token_idx].byte_end()
    } else {
        0
    };

    let change_end_byte = if has_ctx_after {
        let first_ctx_after_sig = match_start_in_sig + removed_len;
        if first_ctx_after_sig < sig_to_token_idx.len() {
            let token_idx = sig_to_token_idx[first_ctx_after_sig];
            tokens[token_idx].start
        } else {
            // context_after があるが sig 列の末尾に達した場合はテキスト末尾
            tokens.last().map(|t| t.byte_end()).unwrap_or(0)
        }
    } else {
        tokens.last().map(|t| t.byte_end()).unwrap_or(0)
    };

    (change_start_byte, change_end_byte)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;
    use crate::patch::context::ContextConfig;
    use crate::patch::generator::generate_patch;

    #[test]
    fn test_apply_patch_basic() {
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();

        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();
        let result = apply_patch(orig, &patch, &JAVA).unwrap();

        // 適用後は edited と同じ significant tokens を持つはず
        use crate::lexer::GenericTokenizer;
        let tokenizer = GenericTokenizer::new(&JAVA);
        let result_sig: Vec<String> = tokenizer
            .tokenize(&result)
            .into_iter()
            .filter(|t| t.is_significant())
            .map(|t| t.text.clone())
            .collect();
        let edit_sig: Vec<String> = tokenizer
            .tokenize(edit)
            .into_iter()
            .filter(|t| t.is_significant())
            .map(|t| t.text.clone())
            .collect();

        assert_eq!(result_sig, edit_sig);
    }

    #[test]
    fn test_apply_patch_verbatim() {
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();

        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();
        let result = apply_patch(orig, &patch, &JAVA).unwrap();

        assert_eq!(result, edit);
    }

    #[test]
    fn test_apply_patch_tab_indent() {
        let orig = "void foo() {\n\treturn null;\n}\n";
        let edit = "void foo() {\n\tObjects.requireNonNull(bar);\n\treturn null;\n}\n";
        let config = ContextConfig::default();

        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();
        let result = apply_patch(orig, &patch, &JAVA).unwrap();

        assert_eq!(result, edit);
    }

    #[test]
    fn test_apply_patch_line_shifted() {
        // 行番号が変わっても適用できることを確認
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        // 行が追加された状態に適用
        let shifted = "// added line\nvoid foo() {\n    return null;\n}\n";
        let result = apply_patch(shifted, &patch, &JAVA);
        assert!(result.is_ok(), "行シフト後も適用できること: {:?}", result);
    }

    #[test]
    fn test_apply_patch_multiple_matches() {
        use crate::lexer::token::TokenKind;
        use crate::lexer::Token;
        use crate::patch::model::{DiffHunk, PatchFile, PatchKind};

        fn tok(text: &str) -> Token {
            Token::new(TokenKind::Code, text)
        }

        let orig = "void foo() { return null; } void bar() { return null; }";
        let patch = PatchFile {
            version: "1".to_string(),
            id: "test".to_string(),
            author: "test".to_string(),
            created_at: "2026-01-01".to_string(),
            description: "test".to_string(),
            target_file: "Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
            hunks: vec![DiffHunk {
                context_before: vec![tok("{")],
                removed: vec![tok("return"), tok("null")],
                added_text: " return 0".to_string(),
                context_after: vec![tok(";")],
                count: 2,
            }],
        };

        let result = apply_patch(orig, &patch, &JAVA).unwrap();
        assert_eq!(result, "void foo() { return 0; } void bar() { return 0; }");
    }

    #[test]
    fn test_apply_patch_count_mismatch() {
        use crate::lexer::token::TokenKind;
        use crate::lexer::Token;
        use crate::patch::model::{DiffHunk, PatchFile, PatchKind};

        fn tok(text: &str) -> Token {
            Token::new(TokenKind::Code, text)
        }

        let orig = "void foo() { return null; } void bar() { return null; }";
        let patch = PatchFile {
            version: "1".to_string(),
            id: "test".to_string(),
            author: "test".to_string(),
            created_at: "2026-01-01".to_string(),
            description: "test".to_string(),
            target_file: "Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
            hunks: vec![DiffHunk {
                context_before: vec![tok("{")],
                removed: vec![tok("return"), tok("null")],
                added_text: " return 0".to_string(),
                context_after: vec![tok(";")],
                count: 1,
            }],
        };

        let result = apply_patch(orig, &patch, &JAVA);
        assert!(matches!(
            result,
            Err(ApplyError::CountMismatch {
                expected: 1,
                actual: 2,
                ..
            })
        ));
    }

    #[test]
    fn test_apply_patch_comment_rewrite_after_semicolon() {
        // return と ; の間（以降）のコメントを含む範囲のコード+コメント書き換え
        let orig = "int x = 1; // old\n";
        let edit = "int x = 2; // new\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        let result = apply_patch(orig, &patch, &JAVA).unwrap();
        assert_eq!(result, edit);
    }

    #[test]
    fn test_apply_patch_lenient_reapply_is_skipped() {
        // Modify hunk を2回適用: 2回目は全 hunk skipped でテキスト不変
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        let first = apply_patch_lenient(orig, &patch, &JAVA).unwrap();
        assert!(first.skipped_hunks.is_empty());
        assert_eq!(first.text, edit);

        let second = apply_patch_lenient(&first.text, &patch, &JAVA).unwrap();
        assert_eq!(
            second.text, first.text,
            "2回目の適用でテキストが不変であること"
        );
        assert_eq!(
            second.skipped_hunks.len(),
            patch.hunks.len(),
            "全ハンクがスキップされること"
        );
    }

    #[test]
    fn test_apply_patch_lenient_insert_only_reapply_is_skipped() {
        // 挿入のみのハンク（removed が空）
        let orig = "void foo() {\n}\n";
        let edit = "void foo() {\n    log();\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        let first = apply_patch_lenient(orig, &patch, &JAVA).unwrap();
        assert_eq!(first.text, edit);

        let second = apply_patch_lenient(&first.text, &patch, &JAVA).unwrap();
        assert_eq!(second.text, first.text);
        assert_eq!(second.skipped_hunks.len(), patch.hunks.len());
    }

    #[test]
    fn test_apply_patch_lenient_delete_only_reapply_is_skipped() {
        // 削除のみのハンク（added_text が空）
        let orig = "void foo() {\n    log();\n    return;\n}\n";
        let edit = "void foo() {\n    return;\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        let first = apply_patch_lenient(orig, &patch, &JAVA).unwrap();
        assert_eq!(first.text, edit);

        let second = apply_patch_lenient(&first.text, &patch, &JAVA).unwrap();
        assert_eq!(second.text, first.text);
        assert_eq!(second.skipped_hunks.len(), patch.hunks.len());
    }

    #[test]
    fn test_apply_patch_lenient_partial_drift_still_errors() {
        // count=2 のうち片方だけ適用済みという「部分適用」はドリフト扱いで
        // エラーになること（安全条件: マッチ数がちょうど count と一致する場合のみ skip）
        use crate::lexer::token::TokenKind;
        use crate::lexer::Token;
        use crate::patch::model::{DiffHunk, PatchFile, PatchKind};

        fn tok(text: &str) -> Token {
            Token::new(TokenKind::Code, text)
        }

        // 1箇所だけ既に適用済み、もう1箇所は未適用のテキスト
        let partially_applied = "void foo() { return 0; } void bar() { return null; }";
        let patch = PatchFile {
            version: "1".to_string(),
            id: "test".to_string(),
            author: "test".to_string(),
            created_at: "2026-01-01".to_string(),
            description: "test".to_string(),
            target_file: "Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind: PatchKind::Modify,
            old_path: None,
            verify_tokens: None,
            hunks: vec![DiffHunk {
                context_before: vec![tok("{")],
                removed: vec![tok("return"), tok("null")],
                added_text: " return 0".to_string(),
                context_after: vec![tok(";")],
                count: 2,
            }],
        };

        let result = apply_patch_lenient(partially_applied, &patch, &JAVA);
        assert!(
            matches!(result, Err(ApplyError::CountMismatch { .. })),
            "部分適用は CountMismatch になること: {:?}",
            result
        );
    }

    #[test]
    fn test_apply_patch_comment_removal_between_tokens() {
        // return と null の間のブロックコメントを削除
        let orig = "return /* c */ null;\n";
        let edit = "return null;\n";
        let config = ContextConfig::default();
        let patch = generate_patch(
            orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config,
        )
        .unwrap();

        let result = apply_patch(orig, &patch, &JAVA).unwrap();
        assert_eq!(result, edit);
    }
}

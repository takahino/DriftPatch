use crate::lexer::{GenericTokenizer, LanguageProfile, Token, TokenKind};
use crate::patch::context::find_patch_matches;
use crate::patch::model::{DiffHunk, PatchFile};

#[derive(Debug)]
pub enum ApplyError {
    /// 対象箇所が見つからない
    NoMatch { hunk_index: usize },
    /// マッチが複数あり、適用不可
    AmbiguousMatch { hunk_index: usize, match_count: usize, positions: Vec<usize> },
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::NoMatch { hunk_index } => {
                write!(f, "ハンク {} の適用箇所が見つかりませんでした", hunk_index)
            }
            ApplyError::AmbiguousMatch { hunk_index, match_count, positions } => {
                write!(
                    f,
                    "ハンク {} のマッチが {} 箇所あり、適用できません。位置: {:?}",
                    hunk_index, match_count, positions
                )
            }
        }
    }
}

/// パッチを適用してテキストを返す。
/// スライディングウィンドウで significant tokens にマッチし、
/// 元の書式（空白・コメント・改行）を維持しながらテキストを再構築する。
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

fn apply_hunk(
    text: &str,
    hunk_idx: usize,
    hunk: &DiffHunk,
    profile: &'static LanguageProfile,
) -> Result<String, ApplyError> {
    let tokenizer = GenericTokenizer::new(profile);
    let tokens = tokenizer.tokenize(text);
    let sig: Vec<&Token> = tokens.iter().filter(|t| t.is_significant()).collect();

    // significant tokens 内でのマッチ位置を探す
    let matches = find_patch_matches(&sig, &hunk.context_before, &hunk.removed, &hunk.context_after);

    match matches.len() {
        0 => Err(ApplyError::NoMatch { hunk_index: hunk_idx }),
        1 => {
            let match_start_in_sig = matches[0]; // removed の開始
            let match_end_in_sig = match_start_in_sig + hunk.removed.len(); // removed の終了 (exclusive)

            // significant tokens のインデックスを元のトークン列のインデックスへマッピング
            let sig_to_token_idx: Vec<usize> = tokens
                .iter()
                .enumerate()
                .filter(|(_, t)| t.is_significant())
                .map(|(i, _)| i)
                .collect();

            // removed の開始・終了を元のトークン列で探す
            let token_start = sig_to_token_idx[match_start_in_sig];
            let token_end = if match_end_in_sig < sig_to_token_idx.len() {
                sig_to_token_idx[match_end_in_sig]
            } else {
                tokens.len()
            };

            // removed の直前まで空白・コメントを含めてスキャンして、
            // 実際の削除開始位置を少し手前に調整（先行する空白もまとめて削除）
            // → シンプルに: removed の直前の非significant tokensも含む範囲を削除
            let actual_token_start = leading_whitespace_start(&tokens, token_start);
            let actual_token_end = trailing_whitespace_end(&tokens, token_end);

            // テキストを再構築
            let prefix: String = tokens[..actual_token_start].iter().map(|t| t.text.as_str()).collect();
            let suffix: String = tokens[actual_token_end..].iter().map(|t| t.text.as_str()).collect();

            // added トークンをテキスト化（スペースで自然に結合）
            let added_text = tokens_to_text(&hunk.added, &tokens, actual_token_start);

            Ok(format!("{}{}{}", prefix, added_text, suffix))
        }
        n => Err(ApplyError::AmbiguousMatch {
            hunk_index: hunk_idx,
            match_count: n,
            positions: matches,
        }),
    }
}

/// token_start の直前にある連続した空白・改行トークンの開始インデックスを返す。
/// 改行をまたぐ場合は行頭まで戻る。
fn leading_whitespace_start(tokens: &[Token], token_start: usize) -> usize {
    let mut i = token_start;
    // 直前の空白（タブ含む）のみ除去（改行はまたがない）
    while i > 0 {
        match tokens[i - 1].kind {
            TokenKind::Whitespace => i -= 1,
            _ => break,
        }
    }
    i
}

/// token_end の直後にある連続した空白・改行トークンの終了インデックスを返す。
fn trailing_whitespace_end(tokens: &[Token], token_end: usize) -> usize {
    let mut i = token_end;
    // 直後の空白・改行を除去
    while i < tokens.len() {
        match tokens[i].kind {
            TokenKind::Whitespace | TokenKind::Newline => i += 1,
            _ => break,
        }
    }
    i
}

/// added トークン列をテキストとして整形する。
/// コードトークンとして空白で区切って結合し、
/// 元テキストの行頭インデントを引き継ぐようにする。
fn tokens_to_text(added: &[Token], orig_tokens: &[Token], insert_pos: usize) -> String {
    if added.is_empty() {
        return String::new();
    }

    // 挿入位置の行頭インデントを取得
    let indent = detect_indent(orig_tokens, insert_pos);

    // トークンをスペース区切りで結合し、改行があれば適切にインデントを付与する
    // シンプルな実装: 記号トークンはスペースなし、識別子はスペースあり
    let mut result = indent.clone();
    let mut prev_kind: Option<&TokenKind> = None;

    for (i, tok) in added.iter().enumerate() {
        if i == 0 {
            result.push_str(&tok.text);
        } else {
            let need_space = match (prev_kind, &tok.kind) {
                // 記号の前後はスペース不要
                (_, TokenKind::Code) if is_punctuation(&tok.text) => false,
                (Some(TokenKind::Code), _) if prev_is_punctuation(added, i) => false,
                // 識別子間はスペース
                (Some(TokenKind::Code), TokenKind::Code) => true,
                (Some(TokenKind::StringLiteral), TokenKind::Code) => true,
                (Some(TokenKind::Code), TokenKind::StringLiteral) => true,
                _ => false,
            };
            if need_space {
                result.push(' ');
            }
            result.push_str(&tok.text);
        }
        prev_kind = Some(&tok.kind);
    }
    result.push('\n');
    result
}

fn is_punctuation(text: &str) -> bool {
    matches!(
        text,
        "." | "," | ";" | ":" | "(" | ")" | "[" | "]" | "{" | "}" | "<" | ">"
    )
}

fn prev_is_punctuation(tokens: &[Token], i: usize) -> bool {
    if i == 0 { return false; }
    is_punctuation(&tokens[i - 1].text)
}

/// 挿入位置の直前の行頭インデントを検出する
fn detect_indent(tokens: &[Token], insert_pos: usize) -> String {
    // insert_pos の直前の改行を探し、その後の空白を取得
    let mut i = insert_pos.min(tokens.len());
    // 直前の改行を探す
    while i > 0 {
        i -= 1;
        if tokens[i].kind == TokenKind::Newline {
            // この改行の直後の空白トークンを収集
            let mut j = i + 1;
            let mut indent = String::new();
            while j < tokens.len() && tokens[j].kind == TokenKind::Whitespace {
                indent.push_str(&tokens[j].text);
                j += 1;
            }
            return indent;
        }
    }
    String::new()
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

        let patch = generate_patch(orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config).unwrap();
        let result = apply_patch(orig, &patch, &JAVA).unwrap();

        // 適用後は edited と同じ significant tokens を持つはず
        use crate::lexer::GenericTokenizer;
        let tokenizer = GenericTokenizer::new(&JAVA);
        let result_sig: Vec<String> = tokenizer.tokenize(&result)
            .into_iter()
            .filter(|t| t.is_significant())
            .map(|t| t.text.clone())
            .collect();
        let edit_sig: Vec<String> = tokenizer.tokenize(edit)
            .into_iter()
            .filter(|t| t.is_significant())
            .map(|t| t.text.clone())
            .collect();

        assert_eq!(result_sig, edit_sig);
    }

    #[test]
    fn test_apply_patch_line_shifted() {
        // 行番号が変わっても適用できることを確認
        let orig = "void foo() {\n    return null;\n}\n";
        let edit = "void foo() {\n    Objects.requireNonNull(bar);\n    return null;\n}\n";
        let config = ContextConfig::default();
        let patch = generate_patch(orig, edit, &JAVA, "tester", "test", "Foo.java", "UTF-8", &config).unwrap();

        // 行が追加された状態に適用
        let shifted = "// added line\nvoid foo() {\n    return null;\n}\n";
        let result = apply_patch(shifted, &patch, &JAVA);
        assert!(result.is_ok(), "行シフト後も適用できること: {:?}", result);
    }
}

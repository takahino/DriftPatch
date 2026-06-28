use similar::{ChangeTag, TextDiff};
use crate::lexer::Token;

/// significant tokens（CODE + STRING_LITERAL + コメント）のみを取り出す
pub fn extract_significant(tokens: &[Token]) -> Vec<&Token> {
    tokens.iter().filter(|t| t.is_significant()).collect()
}

/// 2つのトークン列の significant tokens を比較し、差分インデックスを返す。
/// 戻り値: (removed_indices_in_a, added_indices_in_b)
///   removed_indices_in_a: a の significant tokens のうち削除されたインデックス
///   added_indices_in_b  : b の significant tokens のうち追加されたインデックス
#[allow(dead_code)]
pub fn diff_significant(
    a_tokens: &[Token],
    b_tokens: &[Token],
) -> (Vec<usize>, Vec<usize>) {
    let a_sig: Vec<&Token> = extract_significant(a_tokens);
    let b_sig: Vec<&Token> = extract_significant(b_tokens);

    let a_texts: Vec<&str> = a_sig.iter().map(|t| t.text.as_str()).collect();
    let b_texts: Vec<&str> = b_sig.iter().map(|t| t.text.as_str()).collect();

    // similar crate の TextDiff を行単位ではなく "words" として使用
    // ただしトークン単位なので結合文字列で比較する
    let a_joined = a_texts.join("\x00");
    let b_joined = b_texts.join("\x00");

    let diff = TextDiff::from_words(&a_joined, &b_joined);

    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut a_idx = 0usize;
    let mut b_idx = 0usize;

    for change in diff.iter_all_changes() {
        // \x00 区切りでトークンの境界を数える
        let count = change.value().split('\x00').filter(|s| !s.is_empty()).count();
        match change.tag() {
            ChangeTag::Delete => {
                for k in 0..count {
                    removed.push(a_idx + k);
                }
                a_idx += count;
            }
            ChangeTag::Insert => {
                for k in 0..count {
                    added.push(b_idx + k);
                }
                b_idx += count;
            }
            ChangeTag::Equal => {
                a_idx += count;
                b_idx += count;
            }
        }
    }

    (removed, added)
}

/// より直接的: significant token の文字列ベースで LCS diff を取る
/// 戻り値: Vec<(old_sig_idx or None, new_sig_idx or None)>
#[derive(Debug, Clone)]
pub struct DiffOp {
    pub tag: DiffTag,
    /// a (original) での significant token インデックス（Delete/Equal）
    pub a_index: Option<usize>,
    /// b (modified) での significant token インデックス（Insert/Equal）
    pub b_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffTag {
    Equal,
    Insert,
    Delete,
}

/// significant tokens（CODE + STRING_LITERAL + コメント）のシンプルな Myers diff
pub fn diff_tokens(a_tokens: &[Token], b_tokens: &[Token]) -> Vec<DiffOp> {
    let a_sig: Vec<&Token> = extract_significant(a_tokens);
    let b_sig: Vec<&Token> = extract_significant(b_tokens);

    let a_texts: Vec<&str> = a_sig.iter().map(|t| t.text.as_str()).collect();
    let b_texts: Vec<&str> = b_sig.iter().map(|t| t.text.as_str()).collect();

    lcs_diff(&a_texts, &b_texts)
}

/// LCS ベースの diff（Myers アルゴリズム相当を similar crate で実装）
fn lcs_diff(a: &[&str], b: &[&str]) -> Vec<DiffOp> {
    // similar の SequenceMatcher 相当
    use similar::capture_diff_slices;
    use similar::Algorithm;

    let ops = capture_diff_slices(Algorithm::Myers, a, b);
    let mut result = Vec::new();

    for op in ops {
        match op {
            similar::DiffOp::Equal { old_index, new_index, len } => {
                for k in 0..len {
                    result.push(DiffOp {
                        tag: DiffTag::Equal,
                        a_index: Some(old_index + k),
                        b_index: Some(new_index + k),
                    });
                }
            }
            similar::DiffOp::Delete { old_index, old_len, .. } => {
                for k in 0..old_len {
                    result.push(DiffOp {
                        tag: DiffTag::Delete,
                        a_index: Some(old_index + k),
                        b_index: None,
                    });
                }
            }
            similar::DiffOp::Insert { new_index, new_len, .. } => {
                for k in 0..new_len {
                    result.push(DiffOp {
                        tag: DiffTag::Insert,
                        a_index: None,
                        b_index: Some(new_index + k),
                    });
                }
            }
            similar::DiffOp::Replace { old_index, old_len, new_index, new_len } => {
                for k in 0..old_len {
                    result.push(DiffOp {
                        tag: DiffTag::Delete,
                        a_index: Some(old_index + k),
                        b_index: None,
                    });
                }
                for k in 0..new_len {
                    result.push(DiffOp {
                        tag: DiffTag::Insert,
                        a_index: None,
                        b_index: Some(new_index + k),
                    });
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::{GenericTokenizer, profiles::JAVA};

    fn tokenize(src: &str) -> Vec<Token> {
        GenericTokenizer::new(&JAVA).tokenize(src)
    }

    #[test]
    fn test_diff_insert() {
        let a = tokenize("return null;");
        let b = tokenize("Objects.requireNonNull(bar); return null;");
        let ops = diff_tokens(&a, &b);
        let inserts: Vec<_> = ops.iter().filter(|o| o.tag == DiffTag::Insert).collect();
        assert!(!inserts.is_empty());
    }

    #[test]
    fn test_diff_no_change() {
        let a = tokenize("int x = 1;");
        let b = tokenize("int x = 1;");
        let ops = diff_tokens(&a, &b);
        assert!(ops.iter().all(|o| o.tag == DiffTag::Equal));
    }
}

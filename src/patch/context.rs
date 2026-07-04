use crate::lexer::Token;

/// コンテキスト拡張の設定
pub struct ContextConfig {
    /// 最小コンテキストトークン数（significant tokens）
    #[allow(dead_code)]
    pub min_context: usize,
    /// 最大コンテキストトークン数
    pub max_context: usize,
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            min_context: 5,
            max_context: 240,
        }
    }
}

/// 段階的コンテキストサイズ（min → max の試行値）
pub const CONTEXT_STEPS: &[usize] = &[5, 10, 20, 30, 60, 120, 240];

/// significant tokens のスライスから前後 n 個を取り出す。
/// `sig_tokens`: 全 significant tokens
/// `change_start`: 変更開始インデックス（sig_tokens 内）
/// `change_end`:   変更終了インデックス（sig_tokens 内、exclusive）
/// `n`: コンテキストトークン数
#[allow(dead_code)]
pub fn collect_context(
    sig_tokens: &[&Token],
    change_start: usize,
    change_end: usize,
    n: usize,
) -> (Vec<Token>, Vec<Token>) {
    let before_start = change_start.saturating_sub(n);
    let after_end = (change_end + n).min(sig_tokens.len());

    let ctx_before: Vec<Token> = sig_tokens[before_start..change_start]
        .iter()
        .map(|t| (*t).clone())
        .collect();
    let ctx_after: Vec<Token> = sig_tokens[change_end..after_end]
        .iter()
        .map(|t| (*t).clone())
        .collect();

    (ctx_before, ctx_after)
}

/// 与えられた context_before / context_after が `all_sig` の中でユニークにマッチするか確認する。
/// 戻り値: マッチした位置のリスト（significant tokens 内のインデックス）
#[allow(dead_code)]
pub fn find_context_matches(
    all_sig: &[&Token],
    context_before: &[Token],
    context_after: &[Token],
) -> Vec<usize> {
    // context_before の末尾が変更直前、context_after の先頭が変更直後
    // → context_before[-1] の次が変更開始位置 i とする
    let cb_len = context_before.len();
    let ca_len = context_after.len();

    let mut matches = Vec::new();

    if cb_len == 0 && ca_len == 0 {
        // コンテキストなしは常に「全体マッチ」とみなし警告
        return matches;
    }

    // スライディングウィンドウで context_before を検索し、
    // その直後に context_after が続く位置を探す
    'outer: for start in 0..all_sig.len() {
        // context_before が収まるか
        if start + cb_len > all_sig.len() {
            break;
        }
        // context_before のマッチ確認
        for k in 0..cb_len {
            if all_sig[start + k].text != context_before[k].text {
                continue 'outer;
            }
        }
        // context_after のマッチ確認
        let after_start = start + cb_len;
        // after_start は変更箇所の直後（変更トークン数分スキップ後）
        // ここでは変更トークン数が不明なので、context_after の開始位置を走査する
        // → changed_len を引数として受け取る形に変更が必要だが、
        //    一意性チェックでは変更長を考慮して呼び出し側で処理する
        // シンプルに: context_before の直後に context_after が連続する箇所を探す
        // （変更トークンを含む範囲を包んで context_before と context_after が一致する位置）
        if after_start + ca_len <= all_sig.len() {
            let mut after_ok = true;
            for k in 0..ca_len {
                if all_sig[after_start + k].text != context_after[k].text {
                    after_ok = false;
                    break;
                }
            }
            if after_ok {
                // context_before の末尾インデックスを変更開始位置として記録
                matches.push(after_start);
            }
        }
    }

    matches
}

/// context_before の末尾から変更直前、context_after の先頭から変更直後を考慮した
/// 実際のパッチ適用用マッチング。変更箇所（removed）を挟む形でコンテキストを検索する。
pub fn find_patch_matches(
    all_sig: &[&Token],
    context_before: &[Token],
    removed: &[Token],
    context_after: &[Token],
) -> Vec<usize> {
    let cb_len = context_before.len();
    let rm_len = removed.len();
    let ca_len = context_after.len();
    let total = cb_len + rm_len + ca_len;

    let mut matches = Vec::new();

    if all_sig.len() < total {
        return matches;
    }

    'outer: for start in 0..=(all_sig.len() - total) {
        // context_before
        for k in 0..cb_len {
            if all_sig[start + k].text != context_before[k].text {
                continue 'outer;
            }
        }
        // removed
        for k in 0..rm_len {
            if all_sig[start + cb_len + k].text != removed[k].text {
                continue 'outer;
            }
        }
        // context_after
        for k in 0..ca_len {
            if all_sig[start + cb_len + rm_len + k].text != context_after[k].text {
                continue 'outer;
            }
        }
        // 変更開始位置（all_sig 内インデックス）を記録
        matches.push(start + cb_len);
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::token::TokenKind;
    use crate::lexer::{profiles::JAVA, GenericTokenizer, Token};

    fn tok(text: &str) -> Token {
        Token::new(TokenKind::Code, text)
    }

    #[test]
    fn test_find_patch_matches_unique() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let src = "void foo() { return null; }";
        let all_tokens = tokenizer.tokenize(src);
        let all_sig: Vec<&Token> = all_tokens.iter().filter(|t| t.is_significant()).collect();

        // sig = ["void", "foo", "(", ")", "{", "return", "null", ";", "}"]
        // context_before は変更直前まで（"{" を含む）
        let ctx_before = vec![tok("foo"), tok("("), tok(")"), tok("{")];
        let removed = vec![tok("return"), tok("null")];
        let ctx_after = vec![tok(";"), tok("}")];

        let m = find_patch_matches(&all_sig, &ctx_before, &removed, &ctx_after);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn test_find_patch_matches_multiple() {
        // 同じパターンが2箇所ある場合
        let tokenizer = GenericTokenizer::new(&JAVA);
        let src = "void foo() { return null; } void bar() { return null; }";
        let all_tokens = tokenizer.tokenize(src);
        let all_sig: Vec<&Token> = all_tokens.iter().filter(|t| t.is_significant()).collect();

        let ctx_before = vec![tok("{")];
        let removed = vec![tok("return"), tok("null")];
        let ctx_after = vec![tok(";")];

        let m = find_patch_matches(&all_sig, &ctx_before, &removed, &ctx_after);
        assert_eq!(m.len(), 2);
    }
}

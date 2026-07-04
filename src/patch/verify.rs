use crate::lexer::{GenericTokenizer, LanguageProfile};

/// テキストの significant token テキスト列を返す。
/// 空白・改行は含まれないため、インデントや改行コードの差を無視した内容比較に使える。
pub fn significant_token_texts(text: &str, profile: &'static LanguageProfile) -> Vec<String> {
    let tokenizer = GenericTokenizer::new(profile);
    tokenizer
        .tokenize(text)
        .into_iter()
        .filter(|t| t.is_significant())
        .map(|t| t.text)
        .collect()
}

/// 内容検証の不一致情報（エラーメッセージでの診断用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyMismatch {
    pub expected_len: usize,
    pub actual_len: usize,
    /// 最初に食い違ったトークン位置（significant token 列上のインデックス）
    pub first_diff_index: Option<usize>,
}

impl std::fmt::Display for VerifyMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "期待トークン数 {} / 実際 {}, 最初の相違位置: {}",
            self.expected_len,
            self.actual_len,
            self.first_diff_index
                .map(|i| i.to_string())
                .unwrap_or_else(|| "-".to_string())
        )
    }
}

/// テキストの significant token 列が expected と全一致するか検証する。
/// 空白・改行・インデントの差は許容し、コード実質（コメント含む）が同じ場合のみ Ok。
pub fn verify_significant_tokens(
    text: &str,
    profile: &'static LanguageProfile,
    expected: &[String],
) -> Result<(), VerifyMismatch> {
    let actual = significant_token_texts(text, profile);
    if actual == expected {
        return Ok(());
    }

    let first_diff_index = actual
        .iter()
        .zip(expected.iter())
        .position(|(a, e)| a != e)
        .or(Some(actual.len().min(expected.len())));

    Err(VerifyMismatch {
        expected_len: expected.len(),
        actual_len: actual.len(),
        first_diff_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;

    #[test]
    fn test_verify_ignores_whitespace_and_indent() {
        let recorded = "class Foo {\n    void a() {}\n}\n";
        let expected = significant_token_texts(recorded, &JAVA);

        // インデント変更・CRLF 化・空白追加でも一致すること
        let drifted_ws = "class Foo {\r\n\tvoid a()  {}\r\n}\r\n";
        assert!(verify_significant_tokens(drifted_ws, &JAVA, &expected).is_ok());
    }

    #[test]
    fn test_verify_detects_code_change() {
        let recorded = "class Foo {\n    void a() {}\n}\n";
        let expected = significant_token_texts(recorded, &JAVA);

        let changed = "class Foo {\n    void b() {}\n}\n";
        let err = verify_significant_tokens(changed, &JAVA, &expected).unwrap_err();
        assert_eq!(err.expected_len, err.actual_len);
        // "class" "Foo" "{" "void" の次の識別子が相違点
        assert_eq!(err.first_diff_index, Some(4));
    }

    #[test]
    fn test_verify_detects_comment_change() {
        // コメントは significant なのでコメント差も不一致になること
        let recorded = "int x = 1; // old\n";
        let expected = significant_token_texts(recorded, &JAVA);

        let changed = "int x = 1; // new\n";
        assert!(verify_significant_tokens(changed, &JAVA, &expected).is_err());
    }

    #[test]
    fn test_verify_detects_length_difference() {
        let recorded = "int x = 1;\n";
        let expected = significant_token_texts(recorded, &JAVA);

        let longer = "int x = 1;\nint y = 2;\n";
        let err = verify_significant_tokens(longer, &JAVA, &expected).unwrap_err();
        assert!(err.actual_len > err.expected_len);
        assert_eq!(err.first_diff_index, Some(expected.len()));
    }
}

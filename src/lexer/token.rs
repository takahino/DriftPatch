use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TokenKind {
    LineComment,
    BlockComment,
    StringLiteral,
    Newline,
    Whitespace,
    Code,
}

impl TokenKind {
    /// パッチマッチング時に意味のあるトークンか（空白・コメントを除く）
    pub fn is_significant(&self) -> bool {
        matches!(self, TokenKind::Code | TokenKind::StringLiteral)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    /// ソース先頭からの byte offset（シリアライズ対象外）
    #[serde(skip)]
    pub start: usize,
}

impl Token {
    pub fn new(kind: TokenKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            start: 0,
        }
    }

    pub fn with_start(kind: TokenKind, text: impl Into<String>, start: usize) -> Self {
        Self {
            kind,
            text: text.into(),
            start,
        }
    }

    pub fn is_significant(&self) -> bool {
        self.kind.is_significant()
    }

    pub fn byte_end(&self) -> usize {
        self.start + self.text.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_significant() {
        assert!(Token::new(TokenKind::Code, "foo").is_significant());
        assert!(Token::new(TokenKind::StringLiteral, "\"bar\"").is_significant());
        assert!(!Token::new(TokenKind::Whitespace, " ").is_significant());
        assert!(!Token::new(TokenKind::Newline, "\n").is_significant());
        assert!(!Token::new(TokenKind::LineComment, "// x").is_significant());
        assert!(!Token::new(TokenKind::BlockComment, "/* x */").is_significant());
    }
}

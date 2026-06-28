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
}

impl Token {
    pub fn new(kind: TokenKind, text: impl Into<String>) -> Self {
        Self { kind, text: text.into() }
    }

    pub fn is_significant(&self) -> bool {
        self.kind.is_significant()
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

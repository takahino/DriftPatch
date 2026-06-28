use super::profiles::LanguageProfile;
use super::token::{Token, TokenKind};

/// 汎用正規表現ベースのトークナイザー。
/// フルパーサー不要。コメント・文字列・空白・改行・コードの5種に分割する。
pub struct GenericTokenizer<'a> {
    profile: &'a LanguageProfile,
}

impl<'a> GenericTokenizer<'a> {
    pub fn new(profile: &'a LanguageProfile) -> Self {
        Self { profile }
    }

    pub fn tokenize(&self, src: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let chars: Vec<char> = src.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut byte_pos = 0;

        'outer: while i < len {
            // 改行
            if chars[i] == '\n' {
                tokens.push(Token::with_start(TokenKind::Newline, "\n", byte_pos));
                byte_pos += 1;
                i += 1;
                continue;
            }
            if chars[i] == '\r' {
                if i + 1 < len && chars[i + 1] == '\n' {
                    tokens.push(Token::with_start(TokenKind::Newline, "\r\n", byte_pos));
                    byte_pos += 2;
                    i += 2;
                } else {
                    tokens.push(Token::with_start(TokenKind::Newline, "\r", byte_pos));
                    byte_pos += 1;
                    i += 1;
                }
                continue;
            }

            // ブロックコメント
            if let Some((bc_start, bc_end)) = self.profile.block_comment {
                if starts_with_str(&chars, i, bc_start) {
                    let (tok, consumed) =
                        self.read_block_comment(&chars, i, byte_pos, bc_start, bc_end);
                    byte_pos += tok.text.len();
                    tokens.push(tok);
                    i += consumed;
                    continue;
                }
            }

            // 行コメント
            if let Some(lc) = self.profile.line_comment {
                if starts_with_str(&chars, i, lc) {
                    let (tok, consumed) = self.read_line_comment(&chars, i, byte_pos);
                    byte_pos += tok.text.len();
                    tokens.push(tok);
                    i += consumed;
                    continue;
                }
            }

            // Python トリプルクォート文字列（シングル・ダブル両方）
            if self.profile.triple_quote {
                for &delim in self.profile.string_delimiters {
                    let triple = format!("{}{}{}", delim, delim, delim);
                    if starts_with_str(&chars, i, &triple) {
                        let (tok, consumed) = self.read_triple_string(&chars, i, byte_pos, delim);
                        byte_pos += tok.text.len();
                        tokens.push(tok);
                        i += consumed;
                        continue 'outer;
                    }
                }
            }

            // 通常の文字列リテラル
            if self.profile.string_delimiters.contains(&chars[i]) {
                let delim = chars[i];
                let (tok, consumed) = self.read_string(&chars, i, byte_pos, delim);
                byte_pos += tok.text.len();
                tokens.push(tok);
                i += consumed;
                continue;
            }

            // 空白（タブ含む、改行除く）
            if chars[i] == ' ' || chars[i] == '\t' {
                let start = i;
                while i < len && (chars[i] == ' ' || chars[i] == '\t') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                tokens.push(Token::with_start(TokenKind::Whitespace, text.clone(), byte_pos));
                byte_pos += text.len();
                continue;
            }

            // CODE: 上記以外をまとめて1トークンとして読む
            let (tok, consumed) = self.read_code(&chars, i, byte_pos);
            byte_pos += tok.text.len();
            tokens.push(tok);
            i += consumed;
        }

        tokens
    }

    fn read_line_comment(&self, chars: &[char], start: usize, start_byte: usize) -> (Token, usize) {
        let mut i = start;
        while i < chars.len() && chars[i] != '\n' && chars[i] != '\r' {
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        (
            Token::with_start(TokenKind::LineComment, text, start_byte),
            i - start,
        )
    }

    fn read_block_comment(
        &self,
        chars: &[char],
        start: usize,
        start_byte: usize,
        bc_start: &str,
        bc_end: &str,
    ) -> (Token, usize) {
        let mut i = start + bc_start.len();
        while i < chars.len() {
            if starts_with_str(chars, i, bc_end) {
                i += bc_end.len();
                break;
            }
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        (
            Token::with_start(TokenKind::BlockComment, text, start_byte),
            i - start,
        )
    }

    fn read_string(
        &self,
        chars: &[char],
        start: usize,
        start_byte: usize,
        delim: char,
    ) -> (Token, usize) {
        let mut i = start + 1; // 開きデリミタをスキップ
        while i < chars.len() {
            if chars[i] == '\\' {
                i += 2; // エスケープシーケンスをスキップ
                continue;
            }
            if chars[i] == delim {
                i += 1;
                break;
            }
            if chars[i] == '\n' || chars[i] == '\r' {
                // 改行で強制終了（不正な文字列リテラルだが処理継続）
                break;
            }
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        (
            Token::with_start(TokenKind::StringLiteral, text, start_byte),
            i - start,
        )
    }

    fn read_triple_string(
        &self,
        chars: &[char],
        start: usize,
        start_byte: usize,
        delim: char,
    ) -> (Token, usize) {
        let triple = format!("{}{}{}", delim, delim, delim);
        let mut i = start + 3; // 開きトリプルクォートをスキップ
        while i < chars.len() {
            if starts_with_str(chars, i, &triple) {
                i += 3;
                break;
            }
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        (
            Token::with_start(TokenKind::StringLiteral, text, start_byte),
            i - start,
        )
    }

    /// CODE トークンを読む。
    /// 識別子（英数字・アンダースコア）はまとめて1トークン。
    /// それ以外の記号は1文字ずつトークン化する。
    fn read_code(&self, chars: &[char], start: usize, start_byte: usize) -> (Token, usize) {
        let ch = chars[start];
        if ch.is_alphanumeric() || ch == '_' {
            let mut i = start;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            (
                Token::with_start(TokenKind::Code, text, start_byte),
                i - start,
            )
        } else {
            // 記号類は1文字ずつ
            (
                Token::with_start(TokenKind::Code, ch.to_string(), start_byte),
                1,
            )
        }
    }
}

/// ヘルパー: chars[i..] が文字列 s で始まるか確認する
pub fn starts_with_str(chars: &[char], i: usize, s: &str) -> bool {
    let s_chars: Vec<char> = s.chars().collect();
    if i + s_chars.len() > chars.len() {
        return false;
    }
    chars[i..i + s_chars.len()] == s_chars[..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::profiles::JAVA;

    #[test]
    fn test_tokenize_simple_java() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let tokens = tokenizer.tokenize("int x = 1;");
        let sig: Vec<&str> = tokens
            .iter()
            .filter(|t| t.is_significant())
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(sig, vec!["int", "x", "=", "1", ";"]);
    }

    #[test]
    fn test_tokenize_byte_offsets() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let src = "int x;\n";
        let tokens = tokenizer.tokenize(src);
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[0].text, "int");
        assert_eq!(tokens[0].byte_end(), 3);
        let newline = tokens.iter().find(|t| t.kind == TokenKind::Newline).unwrap();
        assert_eq!(newline.start, 6);
        assert_eq!(newline.text, "\n");
    }

    #[test]
    fn test_tokenize_line_comment() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let tokens = tokenizer.tokenize("x = 1; // comment");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::LineComment));
    }

    #[test]
    fn test_tokenize_string_literal() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let tokens = tokenizer.tokenize("String s = \"hello\";");
        let strings: Vec<&str> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::StringLiteral)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(strings, vec!["\"hello\""]);
    }

    #[test]
    fn test_tokenize_block_comment() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        let tokens = tokenizer.tokenize("/* block */ int x;");
        assert!(tokens.iter().any(|t| t.kind == TokenKind::BlockComment));
    }

    #[test]
    fn test_tokenize_ignores_whitespace_in_significant() {
        let tokenizer = GenericTokenizer::new(&JAVA);
        // コメントや空白が混在しても meaningful tokens は同じ
        let tokens1 = tokenizer.tokenize("int x=1;");
        let tokens2 = tokenizer.tokenize("int  x  =  1 ; // ignored");
        let sig1: Vec<&str> = tokens1.iter().filter(|t| t.is_significant()).map(|t| t.text.as_str()).collect();
        let sig2: Vec<&str> = tokens2.iter().filter(|t| t.is_significant()).map(|t| t.text.as_str()).collect();
        assert_eq!(sig1, sig2);
    }
}

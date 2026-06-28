use serde::{Deserialize, Serialize};
use crate::lexer::Token;

/// パッチファイル全体（.dpatch の JSON ルート）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchFile {
    pub version: String,
    pub id: String,
    pub author: String,
    pub created_at: String,
    pub description: String,
    pub target_file: String,
    pub language: String,
    pub encoding: String,
    pub hunks: Vec<DiffHunk>,
}

/// 1つの差分ハンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// 変更箇所の直前のコンテキスト（significant tokens のみ）
    pub context_before: Vec<Token>,
    /// 削除されるトークン
    pub removed: Vec<Token>,
    /// 追加されるトークン
    pub added: Vec<Token>,
    /// 変更箇所の直後のコンテキスト（significant tokens のみ）
    pub context_after: Vec<Token>,
}

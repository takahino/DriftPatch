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

fn default_count() -> usize {
    1
}

/// 1つの差分ハンク
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// 変更箇所の直前のコンテキスト（significant tokens のみ）
    pub context_before: Vec<Token>,
    /// 削除されるトークン
    pub removed: Vec<Token>,
    /// 編集後ソースから verbatim 抽出した置換文字列（改行・タブ・コメント含む）
    pub added_text: String,
    /// 変更箇所の直後のコンテキスト（significant tokens のみ）
    pub context_after: Vec<Token>,
    /// このハンクが適用されるべきマッチ数。適用時に実際のマッチ数と一致する必要がある。
    #[serde(default = "default_count")]
    pub count: usize,
}

use crate::lexer::Token;
use serde::{Deserialize, Serialize};

/// 生成時に書き込む .dpatch フォーマットバージョン。
/// "2": kind / old_path / verify_tokens フィールドを追加（v1 は Modify のみ）。
pub const PATCH_FORMAT_VERSION: &str = "2";

/// 読み込み可能なフォーマットバージョン
pub const SUPPORTED_VERSIONS: &[&str] = &["1", "2"];

/// パッチの変更種別。
/// 旧フォーマット（フィールドなし）は serde default により Modify として読まれる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchKind {
    #[default]
    Modify,
    Create,
    Delete,
    Rename,
}

impl PatchKind {
    /// GUI / レポート表示用のラベル（現在の言語で解決される）
    pub fn label(&self) -> &'static str {
        use crate::i18n::tr;
        match self {
            PatchKind::Modify => tr("kind.modify"),
            PatchKind::Create => tr("kind.create"),
            PatchKind::Delete => tr("kind.delete"),
            PatchKind::Rename => tr("kind.rename"),
        }
    }
}

/// パッチファイル全体（.dpatch の JSON ルート）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchFile {
    pub version: String,
    pub id: String,
    pub author: String,
    pub created_at: String,
    pub description: String,
    /// 対象ファイル（work_dir 相対・`/` 区切り）。Rename では移動後の新パス。
    pub target_file: String,
    pub language: String,
    pub encoding: String,
    /// 変更種別。旧フォーマット（フィールドなし）は Modify。
    #[serde(default)]
    pub kind: PatchKind,
    /// Rename のみ: 移動前の旧パス（work_dir 相対・`/` 区切り）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Delete / 純Rename のみ: 記録時点のファイルの significant token テキスト列。
    /// 適用時に現物と全一致する場合のみ削除・移動を行う（ドリフト検出）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify_tokens: Option<Vec<String>>,
    pub hunks: Vec<DiffHunk>,
}

impl PatchFile {
    /// kind と付随フィールドの整合性を検証する。
    /// 不正なパッチによる誤削除・誤リネームを防ぐため、保存前・適用前に呼ぶ。
    pub fn validate(&self) -> Result<(), String> {
        use crate::i18n::{tr, tr_args};
        match self.kind {
            PatchKind::Modify | PatchKind::Create => {
                if self.old_path.is_some() {
                    return Err(tr_args(
                        "model.no_old_path_for_kind",
                        &[("kind", self.kind.label())],
                    ));
                }
            }
            PatchKind::Delete => {
                if self.verify_tokens.is_none() {
                    return Err(tr("model.delete_requires_verify").to_string());
                }
                if !self.hunks.is_empty() {
                    return Err(tr("model.delete_no_hunks").to_string());
                }
                if self.old_path.is_some() {
                    return Err(tr_args(
                        "model.no_old_path_for_kind",
                        &[("kind", self.kind.label())],
                    ));
                }
            }
            PatchKind::Rename => {
                if self.old_path.as_deref().map_or(true, |p| p.is_empty()) {
                    return Err(tr("model.rename_requires_old_path").to_string());
                }
                // 純リネーム（内容変更なし）は verify_tokens で移動前の内容を検証する
                if self.hunks.is_empty() && self.verify_tokens.is_none() {
                    return Err(tr("model.pure_rename_requires_verify").to_string());
                }
            }
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn base_patch(kind: PatchKind) -> PatchFile {
        PatchFile {
            version: PATCH_FORMAT_VERSION.to_string(),
            id: "20260704-test-abc12345".to_string(),
            author: "tester".to_string(),
            created_at: "2026-07-04T10:00:00+0900".to_string(),
            description: "テスト".to_string(),
            target_file: "src/Foo.java".to_string(),
            language: "java".to_string(),
            encoding: "UTF-8".to_string(),
            kind,
            old_path: None,
            verify_tokens: None,
            hunks: vec![],
        }
    }

    #[test]
    fn test_deserialize_legacy_patch_defaults_to_modify() {
        // 旧フォーマット（kind / old_path / verify_tokens なし）が Modify として読めること
        let json = r#"{
            "version": "1",
            "id": "20260628-test-abc12345",
            "author": "tester",
            "created_at": "2026-06-28T10:00:00+0900",
            "description": "legacy",
            "target_file": "src/Foo.java",
            "language": "java",
            "encoding": "UTF-8",
            "hunks": []
        }"#;
        let patch: PatchFile = serde_json::from_str(json).unwrap();
        assert_eq!(patch.kind, PatchKind::Modify);
        assert!(patch.old_path.is_none());
        assert!(patch.verify_tokens.is_none());
    }

    #[test]
    fn test_serialize_delete_patch_roundtrip() {
        let mut patch = base_patch(PatchKind::Delete);
        patch.verify_tokens = Some(vec!["class".to_string(), "Foo".to_string()]);

        let json = serde_json::to_string(&patch).unwrap();
        let loaded: PatchFile = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.kind, PatchKind::Delete);
        assert_eq!(
            loaded.verify_tokens,
            Some(vec!["class".to_string(), "Foo".to_string()])
        );
    }

    #[test]
    fn test_serialize_modify_omits_optional_fields() {
        // Modify パッチの JSON に old_path / verify_tokens キーが出ないこと
        let patch = base_patch(PatchKind::Modify);
        let json = serde_json::to_string(&patch).unwrap();
        assert!(!json.contains("old_path"));
        assert!(!json.contains("verify_tokens"));
    }

    #[test]
    fn test_validate_delete_requires_verify_tokens() {
        let patch = base_patch(PatchKind::Delete);
        assert!(patch.validate().is_err());

        let mut ok = base_patch(PatchKind::Delete);
        ok.verify_tokens = Some(vec!["x".to_string()]);
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn test_validate_rename_requires_old_path() {
        let mut patch = base_patch(PatchKind::Rename);
        patch.verify_tokens = Some(vec!["x".to_string()]);
        assert!(patch.validate().is_err());

        patch.old_path = Some("src/Old.java".to_string());
        assert!(patch.validate().is_ok());
    }

    #[test]
    fn test_validate_pure_rename_requires_verify_tokens() {
        let mut patch = base_patch(PatchKind::Rename);
        patch.old_path = Some("src/Old.java".to_string());
        assert!(
            patch.validate().is_err(),
            "hunks 空で verify_tokens なしはエラー"
        );
    }

    #[test]
    fn test_validate_modify_rejects_old_path() {
        let mut patch = base_patch(PatchKind::Modify);
        patch.old_path = Some("src/Old.java".to_string());
        assert!(patch.validate().is_err());
    }
}

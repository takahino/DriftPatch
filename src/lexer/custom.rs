//! `profiles.json`（設定ディレクトリ）によるカスタム言語プロファイルの読み込み。
//!
//! 起動時に一度だけ `init_custom_profiles` を呼び、パースに成功したプロファイルは
//! `Box::leak` で `'static` 化してレジストリに登録する。組み込みプロファイルは
//! すべて `&'static` 前提で設計されているため（applier / generator / verify など）、
//! この方式なら既存コードを一切変更せずにカスタムプロファイルを混在させられる。

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::i18n::{tr, tr_args};
use crate::lexer::profiles::LanguageProfile;

#[derive(Debug, Deserialize)]
struct CustomProfileDef {
    name: String,
    extensions: Vec<String>,
    #[serde(default)]
    line_comments: Vec<String>,
    #[serde(default)]
    block_comment: Option<[String; 2]>,
    #[serde(default)]
    string_delimiters: Vec<char>,
    #[serde(default)]
    triple_quote: bool,
}

/// JSON テキストをパースするのみ（バリデーションなし）。テストで分離しやすくするため。
fn parse_profiles(json: &str) -> Result<Vec<CustomProfileDef>, String> {
    serde_json::from_str::<Vec<CustomProfileDef>>(json).map_err(|e| e.to_string())
}

/// name の重複・空文字、extensions の空を検出する。
fn validate(defs: &[CustomProfileDef]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for def in defs {
        if def.name.trim().is_empty() {
            return Err(tr("custom_profile.empty_name").to_string());
        }
        if def.extensions.is_empty() {
            return Err(tr_args(
                "custom_profile.empty_extensions",
                &[("name", &def.name)],
            ));
        }
        if !seen.insert(def.name.clone()) {
            return Err(tr_args(
                "custom_profile.duplicate_name",
                &[("name", &def.name)],
            ));
        }
    }
    Ok(())
}

/// owned な定義を `Box::leak` で `'static LanguageProfile` へ変換する。
/// 起動時に一度だけ呼ばれる想定（リークは有限）。
fn leak_profile(def: CustomProfileDef) -> &'static LanguageProfile {
    let name: &'static str = Box::leak(def.name.into_boxed_str());

    let extensions: Vec<&'static str> = def
        .extensions
        .into_iter()
        .map(|e| -> &'static str { Box::leak(e.into_boxed_str()) })
        .collect();
    let extensions: &'static [&'static str] = Box::leak(extensions.into_boxed_slice());

    let line_comments: Vec<&'static str> = def
        .line_comments
        .into_iter()
        .map(|e| -> &'static str { Box::leak(e.into_boxed_str()) })
        .collect();
    let line_comments: &'static [&'static str] = Box::leak(line_comments.into_boxed_slice());

    let block_comment: Option<(&'static str, &'static str)> = def.block_comment.map(|[s, e]| {
        let s: &'static str = Box::leak(s.into_boxed_str());
        let e: &'static str = Box::leak(e.into_boxed_str());
        (s, e)
    });

    let string_delimiters: &'static [char] = Box::leak(def.string_delimiters.into_boxed_slice());

    Box::leak(Box::new(LanguageProfile {
        name,
        extensions,
        line_comments,
        block_comment,
        string_delimiters,
        triple_quote: def.triple_quote,
    }))
}

static CUSTOM_PROFILES: OnceLock<Vec<&'static LanguageProfile>> = OnceLock::new();

fn profiles_json_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("DriftPatch").join("profiles.json"))
}

/// 起動時に一度だけ呼ぶ。`profiles.json` が存在しなければ何もせず `None` を返す。
/// パース・バリデーションに失敗した場合は警告メッセージを返し、組み込みプロファイルの
/// みで続行できるようにする（起動は止めない）。
pub fn init_custom_profiles() -> Option<String> {
    let Some(path) = profiles_json_path() else {
        let _ = CUSTOM_PROFILES.set(Vec::new());
        return None;
    };

    if !path.exists() {
        let _ = CUSTOM_PROFILES.set(Vec::new());
        return None;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            let _ = CUSTOM_PROFILES.set(Vec::new());
            return Some(tr_args(
                "custom_profile.read_error",
                &[
                    ("path", &path.display().to_string()),
                    ("err", &e.to_string()),
                ],
            ));
        }
    };

    match parse_profiles(&content).and_then(|defs| validate(&defs).map(|()| defs)) {
        Ok(defs) => {
            let leaked: Vec<&'static LanguageProfile> =
                defs.into_iter().map(leak_profile).collect();
            let _ = CUSTOM_PROFILES.set(leaked);
            None
        }
        Err(e) => {
            let _ = CUSTOM_PROFILES.set(Vec::new());
            Some(tr_args(
                "custom_profile.parse_error",
                &[("path", &path.display().to_string()), ("err", &e)],
            ))
        }
    }
}

/// 登録済みのカスタムプロファイル一覧（未初期化なら空）。
pub fn custom_profiles() -> &'static [&'static LanguageProfile] {
    CUSTOM_PROFILES.get().map(Vec::as_slice).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_profiles_basic() {
        let json = r##"[
            {
                "name": "hcl",
                "extensions": ["tf", "hcl"],
                "line_comments": ["#", "//"],
                "string_delimiters": ["\""]
            }
        ]"##;
        let defs = parse_profiles(json).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "hcl");
        assert_eq!(defs[0].extensions, vec!["tf", "hcl"]);
        assert_eq!(defs[0].line_comments, vec!["#", "//"]);
        assert_eq!(defs[0].block_comment, None);
        assert!(!defs[0].triple_quote);
    }

    #[test]
    fn test_parse_profiles_invalid_json_errors() {
        let result = parse_profiles("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_rejects_empty_extensions() {
        let defs = vec![CustomProfileDef {
            name: "x".to_string(),
            extensions: vec![],
            line_comments: vec![],
            block_comment: None,
            string_delimiters: vec![],
            triple_quote: false,
        }];
        assert!(validate(&defs).is_err());
    }

    #[test]
    fn test_validate_rejects_duplicate_name() {
        let defs = vec![
            CustomProfileDef {
                name: "x".to_string(),
                extensions: vec!["x".to_string()],
                line_comments: vec![],
                block_comment: None,
                string_delimiters: vec![],
                triple_quote: false,
            },
            CustomProfileDef {
                name: "x".to_string(),
                extensions: vec!["y".to_string()],
                line_comments: vec![],
                block_comment: None,
                string_delimiters: vec![],
                triple_quote: false,
            },
        ];
        assert!(validate(&defs).is_err());
    }

    #[test]
    fn test_validate_accepts_well_formed_defs() {
        let defs = vec![CustomProfileDef {
            name: "hcl".to_string(),
            extensions: vec!["tf".to_string()],
            line_comments: vec!["#".to_string()],
            block_comment: None,
            string_delimiters: vec!['"'],
            triple_quote: false,
        }];
        assert!(validate(&defs).is_ok());
    }

    #[test]
    fn test_leak_profile_preserves_fields() {
        let def = CustomProfileDef {
            name: "hcl".to_string(),
            extensions: vec!["tf".to_string(), "hcl".to_string()],
            line_comments: vec!["#".to_string()],
            block_comment: Some(["/*".to_string(), "*/".to_string()]),
            string_delimiters: vec!['"'],
            triple_quote: false,
        };
        let profile = leak_profile(def);
        assert_eq!(profile.name, "hcl");
        assert_eq!(profile.extensions, &["tf", "hcl"]);
        assert_eq!(profile.line_comments, &["#"]);
        assert_eq!(profile.block_comment, Some(("/*", "*/")));
        assert_eq!(profile.string_delimiters, &['"']);
    }
}

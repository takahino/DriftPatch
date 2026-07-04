//! キーベースの i18n カタログ。
//!
//! - メッセージは `domain.name` 形式のキーで管理し、言語ごとのカタログ
//!   （`ja.rs` / `en.rs`）から引く。
//! - 言語追加はカタログファイル 1 枚と `Lang` variant の追加で完結する。
//! - パラメータ付きメッセージは `{name}` プレースホルダを実行時置換する
//!   （format! はリテラル要求のため、言語ごとの語順差はカタログ側で吸収する）。
//! - デフォルト言語は日本語。既存のメッセージ文言はそのまま ja カタログに
//!   移してあるため、言語未設定の挙動は従来と完全に一致する。

mod en;
mod ja;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Ja,
    En,
}

impl Lang {
    /// 全言語（カタログ整合性テストで使用）
    pub const ALL: &'static [Lang] = &[Lang::Ja, Lang::En];

    /// 設定ファイル・CLI 引数用の識別子
    pub fn code(&self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }

    fn catalog(&self) -> &'static HashMap<&'static str, &'static str> {
        match self {
            Lang::Ja => catalog_map(&JA_MAP, ja::CATALOG),
            Lang::En => catalog_map(&EN_MAP, en::CATALOG),
        }
    }
}

static JA_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static EN_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn catalog_map(
    cell: &'static OnceLock<HashMap<&'static str, &'static str>>,
    entries: &'static [(&'static str, &'static str)],
) -> &'static HashMap<&'static str, &'static str> {
    cell.get_or_init(|| entries.iter().copied().collect())
}

/// 現在の言語。0 = Ja（デフォルト）, 1 = En
static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::En,
        _ => Lang::Ja,
    }
}

/// "ja" / "en" などの識別子から言語を解決する
pub fn lang_from_str(s: &str) -> Option<Lang> {
    match s.trim().to_ascii_lowercase().as_str() {
        "ja" | "japanese" | "日本語" => Some(Lang::Ja),
        "en" | "english" => Some(Lang::En),
        _ => None,
    }
}

/// 現在の言語でメッセージを引く。
/// キーが無い場合は ja（原文言語）にフォールバックし、それも無ければキーを返す。
pub fn tr(key: &'static str) -> &'static str {
    tr_for(lang(), key)
}

/// 言語明示版（テストは並列実行のためグローバル言語を変更せずこちらを使う）
pub fn tr_for(lang: Lang, key: &'static str) -> &'static str {
    if let Some(v) = lang.catalog().get(key) {
        return v;
    }
    if let Some(v) = Lang::Ja.catalog().get(key) {
        return v;
    }
    key
}

/// `{name}` プレースホルダを置換してメッセージを組み立てる
pub fn tr_args(key: &'static str, args: &[(&str, &str)]) -> String {
    tr_args_for(lang(), key, args)
}

pub fn tr_args_for(lang: Lang, key: &'static str, args: &[(&str, &str)]) -> String {
    let mut text = tr_for(lang, key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{}}}", name), value);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// メッセージ内の `{name}` プレースホルダ集合を抽出する
    fn placeholders(text: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            let Some(len) = rest[start + 1..].find('}') else {
                break;
            };
            set.insert(rest[start + 1..start + 1 + len].to_string());
            rest = &rest[start + 1 + len + 1..];
        }
        set
    }

    #[test]
    fn test_catalogs_have_identical_key_sets() {
        let ja_keys: BTreeSet<_> = ja::CATALOG.iter().map(|(k, _)| *k).collect();
        assert_eq!(ja_keys.len(), ja::CATALOG.len(), "ja カタログにキー重複");

        for lang in Lang::ALL {
            let entries = match lang {
                Lang::Ja => ja::CATALOG,
                Lang::En => en::CATALOG,
            };
            let keys: BTreeSet<_> = entries.iter().map(|(k, _)| *k).collect();
            assert_eq!(keys.len(), entries.len(), "{:?} カタログにキー重複", lang);
            assert_eq!(
                keys, ja_keys,
                "{:?} カタログのキー集合が ja と一致しません",
                lang
            );
        }
    }

    #[test]
    fn test_catalogs_have_identical_placeholders_per_key() {
        for (key, ja_text) in ja::CATALOG {
            let expected = placeholders(ja_text);
            for lang in Lang::ALL {
                let actual = placeholders(tr_for(*lang, key));
                assert_eq!(
                    actual, expected,
                    "キー {} のプレースホルダが {:?} と ja で一致しません",
                    key, lang
                );
            }
        }
    }

    #[test]
    fn test_tr_for_returns_language_specific_text() {
        assert_eq!(
            tr_for(Lang::Ja, "gen.no_diff"),
            "変更が見つかりませんでした"
        );
        assert_eq!(tr_for(Lang::En, "gen.no_diff"), "No changes found");
    }

    #[test]
    fn test_tr_for_falls_back_to_key_when_missing() {
        assert_eq!(tr_for(Lang::En, "no.such.key"), "no.such.key");
    }

    #[test]
    fn test_tr_args_for_replaces_placeholders() {
        let ja = tr_args_for(Lang::Ja, "apply.no_match", &[("hunk", "3")]);
        assert_eq!(ja, "ハンク 3 の適用箇所が見つかりませんでした");
        let en = tr_args_for(Lang::En, "apply.no_match", &[("hunk", "3")]);
        assert_eq!(en, "Hunk 3: no matching location found");
    }

    #[test]
    fn test_lang_from_str() {
        assert_eq!(lang_from_str("ja"), Some(Lang::Ja));
        assert_eq!(lang_from_str("EN"), Some(Lang::En));
        assert_eq!(lang_from_str("fr"), None);
    }
}

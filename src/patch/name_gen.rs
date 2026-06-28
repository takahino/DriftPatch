use chrono::Local;
use uuid::Uuid;

/// パッチIDと作成日時を生成する。
/// 戻り値: (id_string, iso8601_datetime)
/// id 形式: {YYYYMMDD}-{description-kebab}-{uuid8}
pub fn generate_patch_id(description: &str) -> (String, String) {
    let now = Local::now();
    let date = now.format("%Y%m%d").to_string();
    let created_at = now.format("%Y-%m-%dT%H:%M:%S%z").to_string();

    let uuid8 = &Uuid::new_v4().to_string().replace('-', "")[..8];
    let kebab = to_kebab(description);

    let id = if kebab.is_empty() {
        format!("{}-{}", date, uuid8)
    } else {
        format!("{}-{}-{}", date, kebab, uuid8)
    };

    (id, created_at)
}

/// .dpatch ファイル名を生成する
pub fn generate_filename(description: &str) -> String {
    let (id, _) = generate_patch_id(description);
    format!("{}.dpatch", id)
}

/// description を kebab-case に変換する（ASCII のみ）。
/// 日本語はスキップし、英数字のみ使用する。
fn to_kebab(s: &str) -> String {
    let mut result = String::new();
    let mut prev_hyphen = true; // 先頭のハイフンを防ぐ

    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c.to_ascii_lowercase());
            prev_hyphen = false;
        } else if !prev_hyphen && (c == ' ' || c == '_' || c == '-') {
            result.push('-');
            prev_hyphen = true;
        }
        // 日本語・その他は無視
    }

    // 末尾のハイフンを除去
    result.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_patch_id_format() {
        let (id, _) = generate_patch_id("fix null check");
        // {YYYYMMDD}-fix-null-check-{uuid8} の形式
        let parts: Vec<&str> = id.split('-').collect();
        assert!(parts.len() >= 4);
        assert_eq!(parts[0].len(), 8); // YYYYMMDD
    }

    #[test]
    fn test_generate_filename() {
        let name = generate_filename("add logging");
        assert!(name.ends_with(".dpatch"));
        assert!(name.contains("add-logging"));
    }

    #[test]
    fn test_to_kebab_japanese() {
        // 日本語はスキップされる
        let (id, _) = generate_patch_id("要件#123 fix null");
        assert!(id.contains("fix-null"));
    }

    #[test]
    fn test_unique_ids() {
        let (id1, _) = generate_patch_id("test");
        let (id2, _) = generate_patch_id("test");
        // uuid8 部分が異なるので一致しないはず
        assert_ne!(id1, id2);
    }
}

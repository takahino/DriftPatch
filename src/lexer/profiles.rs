use std::path::Path;

#[derive(Debug, Clone)]
pub struct LanguageProfile {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    pub string_delimiters: &'static [char],
    /// Python の ''' / """ トリプルクォート対応
    pub triple_quote: bool,
}

pub const JAVA: LanguageProfile = LanguageProfile {
    name: "java",
    extensions: &["java"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const PYTHON: LanguageProfile = LanguageProfile {
    name: "python",
    extensions: &["py"],
    line_comment: Some("#"),
    block_comment: None,
    string_delimiters: &['"', '\''],
    triple_quote: true,
};

pub const CPP: LanguageProfile = LanguageProfile {
    name: "cpp",
    extensions: &["c", "cpp", "cc", "cxx", "h", "hpp", "hxx", "rc"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const SQL: LanguageProfile = LanguageProfile {
    name: "sql",
    extensions: &["sql"],
    line_comment: Some("--"),
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['\''],
    triple_quote: false,
};

pub const JAVASCRIPT: LanguageProfile = LanguageProfile {
    name: "javascript",
    extensions: &["js", "ts", "jsx", "tsx", "mjs", "cjs"],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\'', '`'],
    triple_quote: false,
};

pub const GENERIC: LanguageProfile = LanguageProfile {
    name: "generic",
    extensions: &[],
    line_comment: Some("//"),
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const ALL_PROFILES: &[&LanguageProfile] = &[
    &JAVA,
    &PYTHON,
    &CPP,
    &SQL,
    &JAVASCRIPT,
];

/// ファイルパスの拡張子からプロファイルを選択する。
/// 一致するものがなければ GENERIC を返す。
pub fn detect_profile(path: &Path) -> &'static LanguageProfile {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    if let Some(ext) = ext {
        for profile in ALL_PROFILES {
            if profile.extensions.contains(&ext.as_str()) {
                return profile;
            }
        }
    }
    &GENERIC
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_detect_java() {
        let p = detect_profile(&PathBuf::from("Foo.java"));
        assert_eq!(p.name, "java");
    }

    #[test]
    fn test_detect_python() {
        let p = detect_profile(&PathBuf::from("script.py"));
        assert_eq!(p.name, "python");
    }

    #[test]
    fn test_detect_generic() {
        let p = detect_profile(&PathBuf::from("file.xyz"));
        assert_eq!(p.name, "generic");
    }
}

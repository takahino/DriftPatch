use std::path::Path;

#[derive(Debug, Clone)]
pub struct LanguageProfile {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    /// 行コメントの開始記号（複数指定可、例: properties の `#` と `!`）
    pub line_comments: &'static [&'static str],
    pub block_comment: Option<(&'static str, &'static str)>,
    pub string_delimiters: &'static [char],
    /// Python の ''' / """ トリプルクォート対応
    pub triple_quote: bool,
}

pub const JAVA: LanguageProfile = LanguageProfile {
    name: "java",
    extensions: &["java"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const PYTHON: LanguageProfile = LanguageProfile {
    name: "python",
    extensions: &["py"],
    line_comments: &["#"],
    block_comment: None,
    string_delimiters: &['"', '\''],
    triple_quote: true,
};

pub const CPP: LanguageProfile = LanguageProfile {
    name: "cpp",
    extensions: &["c", "cpp", "cc", "cxx", "h", "hpp", "hxx", "rc"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const SQL: LanguageProfile = LanguageProfile {
    name: "sql",
    extensions: &["sql"],
    line_comments: &["--"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['\''],
    triple_quote: false,
};

pub const JAVASCRIPT: LanguageProfile = LanguageProfile {
    name: "javascript",
    extensions: &["js", "ts", "jsx", "tsx", "mjs", "cjs"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\'', '`'],
    triple_quote: false,
};

pub const RUST: LanguageProfile = LanguageProfile {
    name: "rust",
    extensions: &["rs"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const CSHARP: LanguageProfile = LanguageProfile {
    name: "csharp",
    extensions: &["cs", "csx"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const GO: LanguageProfile = LanguageProfile {
    name: "go",
    extensions: &["go"],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\'', '`'],
    triple_quote: false,
};

pub const PLSQL: LanguageProfile = LanguageProfile {
    name: "plsql",
    extensions: &["pls", "pks", "pkb", "pck", "psc", "plsql"],
    line_comments: &["--"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['\''],
    triple_quote: false,
};

pub const GENERIC: LanguageProfile = LanguageProfile {
    name: "generic",
    extensions: &[],
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const JSON: LanguageProfile = LanguageProfile {
    name: "json",
    extensions: &["json"],
    line_comments: &[],
    block_comment: None,
    string_delimiters: &['"'],
    triple_quote: false,
};

pub const YAML: LanguageProfile = LanguageProfile {
    name: "yaml",
    extensions: &["yml", "yaml"],
    line_comments: &["#"],
    block_comment: None,
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const PROPERTIES: LanguageProfile = LanguageProfile {
    name: "properties",
    extensions: &["properties"],
    line_comments: &["#", "!"],
    block_comment: None,
    // properties の値中には `'` や `"` が引用符としてではなく素の文字として現れうる
    // （例: it's）ため、誤って文字列リテラル扱いにしないようクォートは無効化する
    string_delimiters: &[],
    triple_quote: false,
};

pub const XML: LanguageProfile = LanguageProfile {
    name: "xml",
    extensions: &["xml", "xsd", "xsl", "xslt", "svg", "xhtml", "html", "htm"],
    line_comments: &[],
    block_comment: Some(("<!--", "-->")),
    string_delimiters: &['"', '\''],
    triple_quote: false,
};

pub const ALL_PROFILES: &[&LanguageProfile] = &[
    &JAVA,
    &PYTHON,
    &CPP,
    &SQL,
    &JAVASCRIPT,
    &RUST,
    &CSHARP,
    &GO,
    &PLSQL,
    &JSON,
    &YAML,
    &PROPERTIES,
    &XML,
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
    fn test_detect_rust() {
        let p = detect_profile(&PathBuf::from("main.rs"));
        assert_eq!(p.name, "rust");
    }

    #[test]
    fn test_detect_csharp() {
        let p = detect_profile(&PathBuf::from("Program.cs"));
        assert_eq!(p.name, "csharp");
    }

    #[test]
    fn test_detect_go() {
        let p = detect_profile(&PathBuf::from("main.go"));
        assert_eq!(p.name, "go");
    }

    #[test]
    fn test_detect_plsql() {
        let p = detect_profile(&PathBuf::from("package.pks"));
        assert_eq!(p.name, "plsql");
        let p2 = detect_profile(&PathBuf::from("spec.plsql"));
        assert_eq!(p2.name, "plsql");
    }

    #[test]
    fn test_detect_generic() {
        let p = detect_profile(&PathBuf::from("file.xyz"));
        assert_eq!(p.name, "generic");
    }

    #[test]
    fn test_detect_json() {
        let p = detect_profile(&PathBuf::from("config.json"));
        assert_eq!(p.name, "json");
    }

    #[test]
    fn test_detect_yaml() {
        let p = detect_profile(&PathBuf::from("docker-compose.yml"));
        assert_eq!(p.name, "yaml");
        let p2 = detect_profile(&PathBuf::from("values.yaml"));
        assert_eq!(p2.name, "yaml");
    }

    #[test]
    fn test_detect_properties() {
        let p = detect_profile(&PathBuf::from("application.properties"));
        assert_eq!(p.name, "properties");
    }

    #[test]
    fn test_detect_xml() {
        let p = detect_profile(&PathBuf::from("pom.xml"));
        assert_eq!(p.name, "xml");
        let p2 = detect_profile(&PathBuf::from("index.html"));
        assert_eq!(p2.name, "xml");
    }
}

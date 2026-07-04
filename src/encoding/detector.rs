use chardetng::EncodingDetector;
use encoding_rs::Encoding;

/// ファイルのバイト列から文字コードを自動判定してUTF-8文字列に変換する。
/// 戻り値: (utf8_string, encoding_name)
pub fn decode_bytes(bytes: &[u8]) -> (String, String) {
    // UTF-8 BOM チェック
    if bytes.starts_with(b"\xef\xbb\xbf") {
        let text = String::from_utf8_lossy(&bytes[3..]).into_owned();
        return (text, "UTF-8 BOM".to_string());
    }

    // chardetng で文字コードを推定
    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);

    // encoding_rs でデコード
    let (decoded, enc_used, _had_errors) = encoding.decode(bytes);
    let enc_name = enc_used.name().to_string();

    (decoded.into_owned(), enc_name)
}

/// テキストを指定エンコーディングでバイト列に変換する。
/// encoding_name が不明な場合は UTF-8 にフォールバックする。
#[allow(dead_code)]
pub fn encode_text(text: &str, encoding_name: &str) -> Vec<u8> {
    let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(encoding_rs::UTF_8);
    let (encoded, _, _) = encoding.encode(text);
    encoded.into_owned()
}

/// ファイルを読み込み、エンコードを自動判定してUTF-8文字列を返す
pub fn read_file_auto(path: &std::path::Path) -> Result<(String, String), std::io::Error> {
    let bytes = std::fs::read(path)?;
    Ok(decode_bytes(&bytes))
}

/// テキストを指定エンコーディングでエンコードしてファイルへ書き込む。
/// encoding_name に "BOM" を含む場合は UTF-8 BOM を先頭に付加する。
pub fn write_file_auto(
    path: &std::path::Path,
    text: &str,
    encoding_name: &str,
) -> std::io::Result<()> {
    let mut bytes = encode_text(text, encoding_name);
    if encoding_name.to_ascii_uppercase().contains("BOM") {
        let mut with_bom = Vec::with_capacity(3 + bytes.len());
        with_bom.extend_from_slice(b"\xef\xbb\xbf");
        with_bom.append(&mut bytes);
        bytes = with_bom;
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_utf8() {
        let src = "Hello, world!";
        let (text, enc) = decode_bytes(src.as_bytes());
        assert_eq!(text, src);
        assert!(enc.to_uppercase().contains("UTF"));
    }

    #[test]
    fn test_decode_utf8_bom() {
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice("test".as_bytes());
        let (text, enc) = decode_bytes(&bytes);
        assert_eq!(text, "test");
        assert!(enc.contains("BOM"));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = "int x = 1;";
        let encoded = encode_text(original, "UTF-8");
        let (decoded, _) = decode_bytes(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_write_file_auto_preserves_utf8_bom() {
        let path =
            std::env::temp_dir().join(format!("driftpatch_bom_{}.txt", uuid::Uuid::new_v4()));
        write_file_auto(&path, "abc", "UTF-8 BOM").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], b"\xef\xbb\xbf");
        assert_eq!(&bytes[3..], b"abc");
        let _ = std::fs::remove_file(&path);
    }
}

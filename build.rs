fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-lib=advapi32");
        println!("cargo:rustc-link-lib=crypt32");
        set_exe_icon();
    }
}

/// `icon.png` から `.ico` を生成し、Windows リソースとして exe に埋め込む。
/// これによりエクスプローラ等で exe 自体のアイコンが表示される。
#[cfg(windows)]
fn set_exe_icon() {
    use image::ImageFormat;
    use std::io::BufWriter;

    println!("cargo:rerun-if-changed=icon.png");

    let png = include_bytes!("icon.png");
    let img = match image::load_from_memory(png) {
        Ok(img) => img,
        Err(e) => {
            println!("cargo:warning=icon.png デコード失敗: {}", e);
            return;
        }
    };

    // ICO 形式は 256x256 以下のためリサイズ
    let img = img.resize(256, 256, image::imageops::FilterType::Lanczos3);

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = std::path::Path::new(&out_dir).join("icon.ico");

    let file = match std::fs::File::create(&ico_path) {
        Ok(f) => f,
        Err(e) => {
            println!("cargo:warning=ICO 出力ファイル作成失敗: {}", e);
            return;
        }
    };
    if let Err(e) = img.write_to(&mut BufWriter::new(file), ImageFormat::Ico) {
        println!("cargo:warning=ICO エンコード失敗: {}", e);
        return;
    }

    let mut res = winres::WindowsResource::new();
    res.set_icon(ico_path.to_str().unwrap());
    if let Err(e) = res.compile() {
        println!("cargo:warning=exeアイコンリソース コンパイル失敗: {}", e);
    }
}

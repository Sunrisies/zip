use std::path::Path;

pub fn should_compress(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // 适合压缩的扩展名
    const COMPRESSIBLE: &[&str] = &[
        "txt",
        "md",
        "csv",
        "json",
        "xml",
        "yaml",
        "yml",
        "html",
        "css",
        "js",
        "log",
        "sql",
        "rs",
        "py",
        "java",
        "cpp",
        "c",
        "h",
        "ini",
        "conf",
        "config",
        "properties",
        "db",
        "sqlite",
        "bmp",
        "tiff",
        "glb",
    ];

    // 不适合压缩的扩展名
    const NON_COMPRESSIBLE: &[&str] = &[
        "pdf", "mp3", "mp4", "avi", "mkv", "mov", "zip", "rar", "7z", "gz", "bz2", "exe", "dll",
        "so", "iso", "dmg", "docx", "xlsx", "pptx", "rlib",
    ];

    if NON_COMPRESSIBLE.contains(&extension.as_str()) {
        true
    } else if COMPRESSIBLE.contains(&extension.as_str()) {
        false
    } else {
        // 对于未知类型，根据文件大小决定
        if let Ok(metadata) = std::fs::metadata(path) {
            // 小于1M的文件不压缩
            metadata.len() > 1024 * 1024
        } else {
            true
        }
    }
}

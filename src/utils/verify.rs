use std::io::Read;
use std::{fs::File, path::Path};
use zip::ZipArchive;
pub fn verify_zip(zip_path: &Path) -> anyhow::Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)?;

    log::info!("验证压缩文件: {:?}", zip_path);
    log::info!("文件数量: {}", archive.len());

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        let size = file.size();
        let method = file.compression();

        log::info!(
            "文件 {}: {} (大小: {}, 压缩方法: {:?})",
            i,
            name,
            size,
            method
        );

        // 验证文件内容
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;

        // 如果需要，可以计算文件校验和
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        buffer.hash(&mut hasher);
        let checksum = hasher.finish();

        log::info!("  校验和: {:x}", checksum);
    }

    Ok(())
}

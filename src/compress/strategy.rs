use crate::utils::file_type::should_compress;
use std::path::Path;
use zip::write::SimpleFileOptions;

pub struct CompressionStrategy {
    level: i64,
}

impl CompressionStrategy {
    pub fn new(level: i64) -> Self {
        Self { level }
    }

    pub fn get_options(&self, path: &Path) -> SimpleFileOptions {
        if should_compress(path) {
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
        } else {
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(self.level))
        }
    }
}

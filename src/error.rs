use thiserror::Error;

#[derive(Error, Debug)]
pub enum ZipError {
    #[error("IO错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("压缩错误: {0}")]
    Compression(String),
    #[error("文件未找到: {0}")]
    FileNotFound(String),
    #[error("路径错误: {0}")]
    PathError(String),
    #[error("线程错误: {0}")]
    ThreadError(String),
}

pub type Result<T> = std::result::Result<T, ZipError>;

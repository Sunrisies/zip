#![allow(unused_variables)]
#![allow(dead_code)]
use clap::{Parser, ValueEnum};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::result::Result::Ok;
use std::sync::mpsc;
use std::time::Instant;
use sunrise_zip::compress::{CompressionStrategy, CompressionTask, CompressionWorker};
use sunrise_zip::utils::{create_progress_bar, verify_zip};
use walkdir::WalkDir;
use zip_lib::ZipWriter;
extern crate zip as zip_lib;
use rayon::prelude::*;
use sunrise_zip::log::init_logger;
#[derive(Parser)]
// #[command(about, long_about = None)]
#[command(about = "压缩文件目录", long_about = Some("将指定目录压缩为 zip 文件"))]
struct Args {
    /// 源目录
    #[arg(help = "源目录的路径")]
    source: PathBuf,
    /// 压缩后的文件名称
    #[arg(help = "目标压缩文件的路径")]
    destination: PathBuf,
    /// 压缩方法
    #[arg(value_enum, default_value_t = CompressionMethod::Deflated, help = "选择压缩方法")]
    compression_method: CompressionMethod,
    #[arg(short = 't', long, default_value_t = 4, help = "压缩线程数")]
    threads: usize,
    #[arg(short = 'l', long, default_value_t = 6, help = "压缩级别 (1-9)")]
    compression_level: i64,
    #[arg(long, default_value_t = 1024, help = "小文件阈值 (KB)")]
    small_file_threshold: u64,
    #[arg(short = 'b', long, help = "是否显示验证信息")]
    verbose: bool,
    /// 显示压缩详情
    #[arg(short = 'd', long, default_value_t = false, help = "显示压缩详情")]
    detail: bool,
}
#[derive(Debug)]
struct CompressedFile {
    index: usize,
    name: String,
    data: Vec<u8>,
    is_compressed: bool,
    is_directory: bool,
}
#[derive(Clone, ValueEnum)]
enum CompressionMethod {
    Stored,
    Deflated,
    DeflatedZlib,
    DeflatedZlibNg,
    Bzip2,
    Zstd,
}

fn main() {
    std::process::exit(match run() {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("错误: {}", e);
            1
        }
    });
}

fn run() -> anyhow::Result<()> {
    init_logger();

    let args = Args::parse();

    // if !args.source.is_dir() {
    //     return Err(ZipError::FileNotFound(args.source.to_string_lossy().to_string()).into());
    // }

    let start = Instant::now();
    compress_directory(
        &args.source,
        &args.destination,
        args.compression_level,
        args.threads,
        args.detail,
    )?;
    println!("压缩完成，耗时: {:?}", start.elapsed());
    // 验证压缩文件
    if args.verbose {
        log::info!("\n验证压缩文件...");
        verify_zip(&args.destination)?;
        log::info!("验证完成！");
    }
    Ok(())
}

fn compress_directory(
    src_dir: &Path,
    dst_file: &Path,
    compression_level: i64,
    num_threads: usize,
    detail: bool,
) -> anyhow::Result<()> {
    // 扫描文件
    let (files, total_size) = {
        let pb = create_progress_bar(None)?;
        let start = Instant::now();
        let mut total_size = 0;

        let files: Vec<_> = WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .inspect(|e| {
                total_size += e.metadata().map(|m| m.len()).unwrap_or(0);
                pb.inc(1);
            })
            .collect();

        pb.finish_and_clear();
        println!(
            "[1/4] 扫描完成: 找到 {} 个文件 ({:.2} {}), 耗时 {:?}",
            files.len(),
            if total_size >= 1024 * 1024 * 1024 {
                total_size as f64 / 1024.0 / 1024.0 / 1024.0
            } else {
                total_size as f64 / 1024.0 / 1024.0
            },
            if total_size >= 1024 * 1024 * 1024 {
                "GB"
            } else {
                "MB"
            },
            start.elapsed()
        );
        (files, total_size)
    };

    // 创建进度条
    let pb = create_progress_bar(Some(files.len() as u64))?;
    println!("压缩管道: {}", num_threads);

    // 创建压缩任务通道
    let (tx, rx) = mpsc::sync_channel::<CompressionTask>(num_threads * 2);

    // 创建压缩工作线程
    let file = std::fs::File::create(dst_file)?;
    let writer = ZipWriter::new(file);
    let strategy = CompressionStrategy::new(compression_level);
    let worker = CompressionWorker::new(writer, strategy, rx, detail);

    match files
        .par_iter()
        .try_for_each(|entry| -> anyhow::Result<()> {
            let path = entry.path();
            let name = path
                .strip_prefix(src_dir)?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("无效路径"))?
                .replace("\\", "/");

            // 根据文件大小选择读取策略
            let data = if entry.metadata()?.len() > 10 * 1024 * 1024 {
                // 10MB
                // 大文件使用内存映射
                let file = std::fs::File::open(path)?;
                let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
                mmap.to_vec()
            } else {
                // 小文件直接读取
                std::fs::read(path)?
            };

            // 获取原始文件大小
            let original_size = if detail { entry.metadata()?.len() } else { 0 };

            // 发送压缩任务
            tx.send(CompressionTask {
                path: path.to_path_buf(),
                name,
                data,
                original_size: Some(original_size),
            })?;

            pb.inc(1);
            Ok(())
        }) {
        Ok(_) => {
            // 正常完成
            drop(tx);
            let (writer, original_total_size) = worker.join()?;
            let mut file = writer.finish()?;
            file.flush()?;
            if detail {
                // 获取压缩后文件大小
                let compressed_size = std::fs::metadata(dst_file)?.len();

                // 计算压缩比
                let compression_ratio = if original_total_size > 0 {
                    (1.0 - compressed_size as f64 / original_total_size as f64) * 100.0
                } else {
                    0.0
                };

                // 显示压缩比
                log::info!("\n压缩统计:");
                log::info!(
                    "原始大小: {:.2} MB",
                    original_total_size as f64 / 1024.0 / 1024.0
                );
                log::info!(
                    "压缩后大小: {:.2} MB",
                    compressed_size as f64 / 1024.0 / 1024.0
                );
                log::info!("压缩比: {:.2}%%", compression_ratio);
            }

            pb.finish_with_message("完成");
        }
        Err(e) => {
            // 发生错误时的清理
            drop(tx);
            pb.abandon_with_message("压缩失败");
            return Err(e);
        }
    }

    Ok(())
}

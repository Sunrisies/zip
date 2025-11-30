#![allow(unused_variables)]
#![allow(dead_code)]
use anyhow::Context;
use clap::{Parser, ValueEnum};
use crossbeam_channel::bounded;
use crossbeam_channel::{Receiver, Sender};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::prelude::*;
use std::io::{BufWriter, Seek, Write};
use std::path::{Path, PathBuf};
use std::result::Result::Ok;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::thread;
use std::time::Instant;
use walkdir::{DirEntry, WalkDir};
use zip::ZipWriter;
use zip::{result::ZipError, write::SimpleFileOptions};
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
    #[arg(short, long, default_value_t = 4, help = "压缩线程数")]
    threads: usize,
    #[arg(short = 'l', long, default_value_t = 1, help = "压缩级别 (1-9)")]
    compression_level: u32,
    #[arg(long, default_value_t = 1024, help = "小文件阈值 (KB)")]
    small_file_threshold: u64,
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
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    if let Err(err) = Args::try_parse() {
        // 输出自定义错误信息
        eprintln!("错误: {}", err);
        eprintln!("使用方法: zip1 <源目录> <目标压缩文件> <压缩方法>");
        eprintln!("更多信息，请使用 '--help'");
        std::process::exit(1);
    }
    let args = Args::parse();
    let src_dir = &args.source;
    let dst_file = &args.destination;
    let method = match args.compression_method {
        CompressionMethod::Stored => zip::CompressionMethod::Stored,
        CompressionMethod::Deflated => {
            #[cfg(not(feature = "deflate-flate2"))]
            {
                println!("未启用“deflet-flate2”功能");
                return 1;
            }
            #[cfg(feature = "deflate-flate2")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::DeflatedZlib => {
            #[cfg(not(feature = "deflate-zlib"))]
            {
                println!("未启用“deflate zlib”功能");
                return 1;
            }
            #[cfg(feature = "deflate-zlib")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::DeflatedZlibNg => {
            #[cfg(not(feature = "deflate-zlib-ng"))]
            {
                println!("未启用“deflate zlib ng”功能");
                return 1;
            }
            #[cfg(feature = "deflate-zlib-ng")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::Bzip2 => {
            #[cfg(not(feature = "bzip2"))]
            {
                println!("未启用bzip2功能");
                return 1;
            }
            #[cfg(feature = "bzip2")]
            zip::CompressionMethod::Bzip2
        }
        CompressionMethod::Zstd => {
            #[cfg(not(feature = "zstd"))]
            {
                println!("zstd功能未启用");
                return 1;
            }
            #[cfg(feature = "zstd")]
            zip::CompressionMethod::Zstd
        }
    };
    let threads = args.threads;

    match doit(
        src_dir,
        dst_file,
        method,
        threads,
        args.compression_level,
        args.small_file_threshold,
    ) {
        Ok(_) => println!("从: {:?} 压缩到 {:?}", src_dir, dst_file),
        Err(e) => eprintln!("压缩失败: {e:?}"),
    }

    0
}

fn zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &Path,
    writer: T,
    method: zip::CompressionMethod,
    total_files: usize,
) -> anyhow::Result<()>
where
    T: Write + Seek,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let prefix = Path::new(prefix);
    let mut buffer = Vec::new();
    let pb = progress_bar_init(Some(total_files as u64))?;

    for entry in it {
        // println!("开始压缩: {:?} 个文件", entry);

        let path = entry.path();
        let name = path
            .strip_prefix(prefix)
            .with_context(|| format!("路径 {:?} 不是前缀 {:?}", prefix, path))?;

        let path_display = path.display().to_string();
        let name_display = name
            .to_str()
            .map(|s| s.replace("\\", "/"))
            .unwrap_or_default();
        let path_as_string = name
            .to_str()
            .map(|s| s.replace("\\", "/"))
            .unwrap_or_default();
        if path.is_file() {
            zip.start_file(&name_display, options)
                .with_context(|| format!("无法将文件 {} 添加到 ZIP 文件", path_display))?;
            let mut f =
                File::open(path).with_context(|| format!("无法打开文件 {}", path_display))?;
            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)
                .with_context(|| format!("无法将文件 {} 写入 ZIP 文件", path_display))?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            zip.add_directory(&name_display, options)
                .with_context(|| format!("无法将目录 {} 添加到 ZIP 文件", name_display))?;
        }
        pb.inc(1);
    }
    zip.finish()?;
    Ok(())
}

fn doit(
    src_dir: &Path,
    dst_file: &Path,
    method: zip::CompressionMethod,
    threads: usize,
    compression_level: u32,
    small_file_threshold: u64,
) -> anyhow::Result<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound.into());
    }
    // 记录开始时间
    let start = Instant::now();
    parallel_compress_optimized(
        src_dir,
        dst_file,
        method,
        threads,
        compression_level,
        small_file_threshold,
    )
    .context("压缩失败")?;
    // 记录结束时间
    let end = Instant::now();
    // 计算并打印压缩所花费的时间
    println!("压缩完成，耗时: {:?}", end - start);
    Ok(())
}

/// 优化的并行压缩实现
fn parallel_compress_optimized(
    src_dir: &Path,
    dst_file: &Path,
    method: zip::CompressionMethod,
    num_threads: usize,
    compression_level: u32,
    small_file_threshold_kb: u64,
) -> anyhow::Result<()> {
    let small_file_threshold = small_file_threshold_kb * 1024;

    println!("[1/4] 扫描文件中...");
    let scan_start = Instant::now();

    // 扫描文件并分类
    let (files, total_size) = scan_files(src_dir)?;
    println!(
        "[1/4] 扫描完成: 找到 {} 个文件 ({:.2} MB), 耗时 {:?}",
        files.len(),
        total_size as f64 / 1024.0 / 1024.0,
        scan_start.elapsed()
    );

    if files.is_empty() {
        return Ok(());
    }

    println!("[2/4] 创建 ZIP 文件...");
    let file = BufWriter::with_capacity(64 * 1024 * 1024, File::create(dst_file)?); // 64MB 缓冲区
    let zip_writer = ZipWriter::new(file);
    let zip = Arc::new(Mutex::new(zip_writer));

    // 创建通信通道
    let (file_tx, file_rx): (Sender<(usize, DirEntry)>, Receiver<(usize, DirEntry)>) =
        bounded(num_threads * 2);

    let (result_tx, result_rx): (Sender<CompressedFile>, Receiver<CompressedFile>) =
        bounded(num_threads * 2);

    // 进度跟踪
    let progress = Arc::new(AtomicUsize::new(0));
    let total_files = files.len();
    let pb = progress_bar_init(Some(total_files as u64))?;
    let progress_clone = Arc::clone(&progress);
    let pb_clone = pb.clone();

    println!("[3/4] 启动 {} 个压缩线程...", num_threads);

    // 启动压缩工作线程
    let compressor_handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let rx = file_rx.clone();
            let tx = result_tx.clone();
            let method = method;
            let src_dir = src_dir.to_path_buf();
            let small_file_threshold = small_file_threshold;

            // 在每个线程中克隆需要的值，而不是移动外部变量
            let thread_progress = Arc::clone(&progress_clone);
            let thread_pb = pb_clone.clone();
            let thread_total_files = total_files;

            std::thread::spawn(move || -> anyhow::Result<()> {
                let mut buffer = Vec::with_capacity(1024 * 1024); // 1MB 缓冲区

                while let Ok((index, entry)) = rx.recv() {
                    let path = entry.path();
                    let name = match path.strip_prefix(&src_dir) {
                        Ok(name) => name
                            .to_str()
                            .map(|s| s.replace("\\", "/"))
                            .unwrap_or_default(),
                        Err(_) => continue,
                    };

                    if path.is_file() {
                        // 根据文件大小选择压缩策略
                        let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        let use_compression = file_size > small_file_threshold;

                        let compressed_data = if use_compression {
                            // 对大文件进行压缩
                            match compress_file_data(path, &method, compression_level, &mut buffer)
                            {
                                Ok(data) => data,
                                Err(e) => {
                                    eprintln!("压缩文件失败 {}: {}", path.display(), e);
                                    continue;
                                }
                            }
                        } else {
                            // 小文件直接存储
                            match read_file_data(path, &mut buffer) {
                                Ok(data) => data,
                                Err(e) => {
                                    eprintln!("读取文件失败 {}: {}", path.display(), e);
                                    continue;
                                }
                            }
                        };

                        let result = CompressedFile {
                            index,
                            name,
                            data: compressed_data,
                            is_compressed: use_compression,
                            is_directory: false,
                        };

                        if let Err(_) = tx.send(result) {
                            break;
                        }
                    } else {
                        // 目录项
                        let result = CompressedFile {
                            index,
                            name,
                            data: Vec::new(),
                            is_compressed: false,
                            is_directory: true,
                        };

                        if let Err(_) = tx.send(result) {
                            break;
                        }
                    }

                    // 更新进度 - 使用线程内部的克隆版本
                    let current = thread_progress.fetch_add(1, Ordering::Relaxed) + 1;
                    if current % 10 == 0 || current == thread_total_files {
                        thread_pb.set_position(current as u64);
                    }
                }
                Ok(())
            })
        })
        .collect();

    println!("[4/4] 开始压缩处理...");

    // 发送文件到工作线程
    for (index, entry) in files.into_iter().enumerate() {
        if let Err(_) = file_tx.send((index, entry)) {
            break;
        }
    }
    drop(file_tx); // 关闭发送通道

    // 写入线程 - 按顺序写入ZIP文件
    let write_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        let mut pending_files = BTreeMap::new();
        let mut next_expected = 0;

        while let Ok(mut compressed_file) = result_rx.recv() {
            pending_files.insert(compressed_file.index, compressed_file);

            // 按顺序处理文件
            while let Some(file) = pending_files.remove(&next_expected) {
                write_file_to_zip(&zip, &file, method, compression_level)?;
                next_expected += 1;
            }
        }

        // 处理剩余文件
        for (_, file) in pending_files {
            write_file_to_zip(&zip, &file, method, compression_level)?;
        }

        Ok(())
    });

    // 等待所有线程完成
    for handle in compressor_handles {
        let _ = handle.join();
    }

    drop(result_tx); // 关闭结果通道

    // 等待写入完成
    write_handle.join().expect("写入线程崩溃")?;

    // 完成ZIP文件
    let zip_writer = Arc::try_unwrap(zip)
        .map_err(|_| anyhow::anyhow!("ZIP writer 仍在使用中"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("锁污染"))?;

    zip_writer.finish()?;
    pb.finish_with_message("压缩完成");

    Ok(())
}

/// 读取文件数据（不压缩）
fn read_file_data(path: &Path, buffer: &mut Vec<u8>) -> anyhow::Result<Vec<u8>> {
    buffer.clear();

    let mut file = File::open(path)?;
    file.read_to_end(buffer)?;

    Ok(buffer.clone())
}

/// 写入文件到ZIP
fn write_file_to_zip(
    zip: &Arc<Mutex<ZipWriter<BufWriter<File>>>>,
    file: &CompressedFile,
    method: zip::CompressionMethod,
    compression_level: u32,
) -> anyhow::Result<()> {
    let mut zip_writer = zip.lock().unwrap();

    let options = SimpleFileOptions::default()
        .compression_method(if file.is_compressed {
            method
        } else {
            zip::CompressionMethod::Stored
        })
        .unix_permissions(0o755);

    if file.is_directory {
        zip_writer.add_directory(&file.name, options)?;
    } else {
        zip_writer.start_file(&file.name, options)?;
        zip_writer.write_all(&file.data)?;
    }

    Ok(())
}

/// 压缩文件数据
fn compress_file_data(
    path: &Path,
    method: &zip::CompressionMethod,
    level: u32,
    buffer: &mut Vec<u8>,
) -> anyhow::Result<Vec<u8>> {
    buffer.clear();

    let mut file = File::open(path)?;
    file.read_to_end(buffer)?;

    // 这里可以添加实际的压缩逻辑
    // 目前只是返回原始数据，实际使用时应根据method和level进行压缩
    Ok(buffer.clone())
}

/// 扫描文件
fn scan_files(src_dir: &Path) -> anyhow::Result<(Vec<DirEntry>, u64)> {
    let mut files = Vec::new();
    let mut total_size = 0;

    for entry in WalkDir::new(src_dir) {
        let entry = entry?;
        let path = entry.path();

        if entry.file_type().is_file() {
            total_size += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }

        files.push(entry);
    }

    // 按文件大小排序，大文件优先处理
    files.sort_by(|a, b| {
        let size_a = a.metadata().map(|m| m.len()).unwrap_or(0);
        let size_b = b.metadata().map(|m| m.len()).unwrap_or(0);
        size_b.cmp(&size_a) // 降序排列
    });

    Ok((files, total_size))
}

fn progress_bar_init(total_files: Option<u64>) -> anyhow::Result<ProgressBar> {
    let pb = match total_files {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };

    let style = match total_files {
        Some(_) => ProgressStyle::default_bar().template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )?,
        None => ProgressStyle::default_spinner()
            .template("{spinner:.green} 已扫描: {pos} 个文件 [{elapsed_precise}]")?,
    };

    pb.set_style(style.progress_chars("#>-"));
    Ok(pb)
}

// fn progress_bar_init(total_files: u64) -> anyhow::Result<ProgressBar> {
//     let pb = ProgressBar::new(total_files);
//     pb.set_style(
//         ProgressStyle::default_bar()
//             .template(
//                 "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
//             )
//             .unwrap()
//             .progress_chars("#>-"),
//     );
//     Ok(pb)
// }

/// 核心压缩逻辑
fn parallel_compress(
    src_dir: &Path,
    dst_file: &Path,
    method: zip::CompressionMethod,
    num_threads: usize,
) -> anyhow::Result<()> {
    let scan_pb = progress_bar_init(None)?;
    let (files, total_size) = {
        let start = Instant::now();

        let mut total_size = 0;
        let files: Vec<_> = WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .inspect(|e| {
                total_size += e.metadata().map(|m| m.len()).unwrap_or(0);
                scan_pb.inc(1); // 更新进度条
            })
            .collect();
        scan_pb.finish_and_clear(); // 完成后清理进度条
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
    let pb = progress_bar_init(Some(files.len() as u64))?;
    println!("压缩管道:{}", num_threads);

    // 创建带缓冲区的ZIP写入器
    let file = BufWriter::with_capacity(1024 * 1024, File::create(dst_file)?);
    let zip_writer = zip::ZipWriter::new(file);
    let zip = Arc::new(RwLock::new(zip_writer));
    let write_lock_interval = 10; // 每处理10个文件释放一次锁
                                  // 原子计数器用于进度跟踪
    let counter = Arc::new(AtomicUsize::new(0));
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    // 使用Rayon全局线程池
    files.par_iter().try_for_each(|entry| {
        let path = entry.path();
        let name = path
            .strip_prefix(src_dir)
            .with_context(|| format!("路径 {:?} 不是前缀 {:?}", src_dir, path))?
            .to_str()
            .map(|s| s.replace("\\", "/"))
            .ok_or_else(|| anyhow::anyhow!("路径包含无效字符"))?;

        // // 内存映射优化
        // let mmap = unsafe { MmapOptions::new().map(&File::open(path)?) }?;

        // // 分段写入锁优化
        // {
        //     let mut writer = zip.write().map_err(|_| anyhow::anyhow!("锁获取失败"))?;
        //     if path.is_file() {
        //         writer.start_file(&name, options)?;
        //         writer.write_all(&mmap)?;
        //     } else if !name.is_empty() {
        //         writer.add_directory(&name, options)?;
        //     }
        // }
        // 修改3：限制锁作用域
        let result = (|| -> anyhow::Result<()> {
            let mmap = unsafe { MmapOptions::new().map(&File::open(path)?) }?;

            // 按间隔获取锁
            if counter.load(Ordering::Relaxed) % write_lock_interval == 0 {
                let mut writer = zip.write().map_err(|_| anyhow::anyhow!("锁获取失败"))?;
                if path.is_file() {
                    writer.start_file(&name, options)?;
                    writer.write_all(&mmap)?;
                }
            } else {
                // 无锁写入（需要确保线程安全）
                let mut writer = zip.write().map_err(|_| anyhow::anyhow!("锁获取失败"))?;
                writer.write_all(&mmap)?;
            }
            // 修改4：强制释放内存映射
            drop(mmap);

            // 原子更新进度
            let prev = counter.fetch_add(1, Ordering::Relaxed);
            if prev % 10 == 0 {
                pb.set_position(prev as u64);
            }
            Ok(())
        })();

        result
    })?;

    // 修改最后的完成方式
    let zip_writer = Arc::try_unwrap(zip)
        .map_err(|_| anyhow::anyhow!("存在未释放的ZIP写入器引用"))?
        .into_inner()
        .map_err(|_| anyhow::anyhow!("锁污染错误"))?;

    let mut file = zip_writer.finish()?;
    file.flush()?;
    pb.finish_with_message("完成");

    // let pb = progress_bar_init(Some(files.len() as u64))?;

    // println!("压缩管道:{}", num_threads);
    // // 创建通信管道
    // let (tx, rx): (Sender<(usize, Vec<u8>)>, Receiver<(usize, Vec<u8>)>) = bounded(num_threads * 2);

    // let dst_file_clone = dst_file.to_path_buf();
    // let file = BufWriter::new(File::create(dst_file).unwrap());
    // let files = Arc::new(files);

    // // 启动写入线程
    // let writer_thread = thread::spawn(move || {
    //     let mut buffer = BTreeMap::new();
    //     // 接收并缓存数据块
    //     for (index, data) in rx {
    //         buffer.insert(index, data);
    //     }
    // });
    // let options = SimpleFileOptions::default()
    //     .compression_method(method)
    //     .unix_permissions(0o755);
    // let zip_file = BufWriter::new(File::create(dst_file).unwrap());
    // let zip = Arc::new(Mutex::new(zip::ZipWriter::new(zip_file)));
    // let next_index = Arc::new(Mutex::new(0));
    // // 并行压缩线程池

    // rayon::ThreadPoolBuilder::new()
    //     .num_threads(num_threads)
    //     .build()
    //     .unwrap()
    //     .install(|| {
    //         files.par_iter().enumerate().for_each(|(index, entry)| {
    //             compress_file(
    //                 zip.clone(),
    //                 entry,
    //                 options,
    //                 &pb,
    //                 src_dir,
    //                 next_index.clone(),
    //             );
    //         });
    //     });

    // // 等待写入完成
    // drop(tx); // 关闭发送端
    // writer_thread.join().unwrap();
    Ok(())
}
// 压缩文件
fn compress_file(
    zip_clone: Arc<Mutex<zip::ZipWriter<BufWriter<File>>>>,
    entry: &DirEntry,
    options: SimpleFileOptions,
    pb: &ProgressBar,
    src_dir: &Path,
    next_index: Arc<Mutex<usize>>,
) {
    let prefix = Path::new(src_dir);
    let path = entry.path();
    let name = path
        .strip_prefix(prefix)
        .with_context(|| format!("路径 {:?} 不是前缀 {:?}", prefix, path))
        .unwrap();
    let mut zip_writer = zip_clone.lock().unwrap();
    let path_display = path.display().to_string();
    let name_display = name
        .to_str()
        .map(|s| s.replace("\\", "/"))
        .unwrap_or_default();
    if path.is_file() {
        // 打印文件大小
        let file = File::open(&path)
            .with_context(|| format!("Failed to open file {}", path_display))
            .unwrap();
        let mmap = unsafe { MmapOptions::new().map(&file) }
            .with_context(|| format!("Failed to map file {}", path_display))
            .unwrap();
        let _ = zip_writer
            .start_file(&name_display, options)
            .with_context(|| format!("无法将文件 {} 添加到 ZIP 文件", path_display));
        let _ = zip_writer
            .write_all(&mmap)
            .with_context(|| format!("无法将文件 {} 写入 ZIP 文件", path_display));
    }
    // else if !name.as_os_str().is_empty() {
    //     // let _ = zip_writer
    //     //     .add_directory(&name_display, options)
    //     //     .with_context(|| format!("无法将目录 {} 添加到 ZIP 文件", name_display));
    // }
    let next_index_clone = Arc::clone(&next_index);
    let mut next = next_index_clone.lock().unwrap();
    *next += 1;
    if *next % 10 == 0 {
        pb.set_position(*next as u64);
    }
}

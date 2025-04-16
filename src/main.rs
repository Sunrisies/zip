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
use zip::{result::ZipError, write::SimpleFileOptions};
#[derive(Parser)]
// #[command(about, long_about = None)]
#[command(about = "压缩文件目录", long_about = Some("将指定目录压缩为 zip 文件"))]
struct Args {
    // 源目录
    #[arg(help = "源目录的路径")]
    source: PathBuf,
    // 压缩后的文件名称
    #[arg(help = "目标压缩文件的路径")]
    destination: PathBuf,
    // 压缩方法
    #[arg(value_enum, default_value_t = CompressionMethod::Deflated, help = "选择压缩方法")]
    compression_method: CompressionMethod,
    #[arg(short, long, default_value_t = 4, help = "压缩线程数")]
    threads: usize,
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

    match doit(src_dir, dst_file, method, threads) {
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
) -> anyhow::Result<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound.into());
    }
    let start1 = Instant::now();
    // let path: &Path = Path::new(dst_file);
    // let file = File::create(path).unwrap();
    // let walkdir = WalkDir::new(path);
    // let it: walkdir::IntoIter = walkdir.into_iter();
    // let walkdir = WalkDir::new(src_dir);
    // let it: walkdir::IntoIter = walkdir.into_iter();
    // let total_files = WalkDir::new(src_dir).into_iter().count();
    let end1 = Instant::now();
    println!("压缩完成，耗时: {:?}", end1 - start1);
    // println!("开始压缩: {:?} 个文件", total_files);
    // 记录开始时间
    let start = Instant::now();
    parallel_compress(src_dir, dst_file, method, threads).context("压缩失败")?;
    // 记录结束时间
    let end = Instant::now();
    // 计算并打印压缩所花费的时间
    println!("压缩完成，耗时: {:?}", end - start);
    Ok(())
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

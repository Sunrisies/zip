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
    #[arg(short, long, default_value_t = 4, help = "压缩线程数")]
    threads: usize,
    #[arg(short = 'l', long, default_value_t = 6, help = "压缩级别 (1-9)")]
    compression_level: i64,
    #[arg(long, default_value_t = 1024, help = "小文件阈值 (KB)")]
    small_file_threshold: u64,
    #[arg(long, help = "是否显示验证信息")]
    verbose: bool,
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
// fn real_main() -> i32 {
//     if let Err(err) = Args::try_parse() {
//         // 输出自定义错误信息
//         eprintln!("错误: {}", err);
//         eprintln!("使用方法: zip1 <源目录> <目标压缩文件> <压缩方法>");
//         eprintln!("更多信息，请使用 '--help'");
//         std::process::exit(1);
//     }
//     let args = Args::parse();
//     let src_dir = &args.source;
//     let dst_file = &args.destination;
//     let method = match args.compression_method {
//         CompressionMethod::Stored => zip::CompressionMethod::Stored,
//         CompressionMethod::Deflated => {
//             #[cfg(not(feature = "deflate-flate2"))]
//             {
//                 println!("未启用“deflet-flate2”功能");
//                 return 1;
//             }
//             #[cfg(feature = "deflate-flate2")]
//             zip::CompressionMethod::Deflated
//         }
//         CompressionMethod::DeflatedZlib => {
//             #[cfg(not(feature = "deflate-zlib"))]
//             {
//                 println!("未启用“deflate zlib”功能");
//                 return 1;
//             }
//             #[cfg(feature = "deflate-zlib")]
//             zip::CompressionMethod::Deflated
//         }
//         CompressionMethod::DeflatedZlibNg => {
//             #[cfg(not(feature = "deflate-zlib-ng"))]
//             {
//                 println!("未启用“deflate zlib ng”功能");
//                 return 1;
//             }
//             #[cfg(feature = "deflate-zlib-ng")]
//             zip::CompressionMethod::Deflated
//         }
//         CompressionMethod::Bzip2 => {
//             #[cfg(not(feature = "bzip2"))]
//             {
//                 println!("未启用bzip2功能");
//                 return 1;
//             }
//             #[cfg(feature = "bzip2")]
//             zip::CompressionMethod::Bzip2
//         }
//         CompressionMethod::Zstd => {
//             #[cfg(not(feature = "zstd"))]
//             {
//                 println!("zstd功能未启用");
//                 return 1;
//             }
//             #[cfg(feature = "zstd")]
//             zip::CompressionMethod::Zstd
//         }
//     };
//     let threads = args.threads;
//     let compression_level = args.compression_level;
//     let small_file_threshold = args.small_file_threshold;
//     match doit(
//         src_dir,
//         dst_file,
//         method,
//         threads,
//         compression_level,
//         small_file_threshold,
//     ) {
//         Ok(_) => println!("从: {:?} 压缩到 {:?}", src_dir, dst_file),
//         Err(e) => eprintln!("压缩失败: {e:?}"),
//     }

//     0
// }

// fn doit(
//     src_dir: &Path,
//     dst_file: &Path,
//     method: zip::CompressionMethod,
//     threads: usize,
//     compression_level: u32,
//     small_file_threshold: u64,
// ) -> anyhow::Result<()> {
//     if !Path::new(src_dir).is_dir() {
//         return Err(ZipError::FileNotFound.into());
//     }
//     // 记录开始时间
//     let start = Instant::now();
//     parallel_compress(
//         src_dir,
//         dst_file,
//         method,
//         threads,
//         compression_level,
//         small_file_threshold,
//     )?;

//     // 记录结束时间
//     let end = start.elapsed();
//     // 计算并打印压缩所花费的时间
//     println!("压缩完成，耗时: {:?}", end);
//     Ok(())
// }

fn compress_directory(
    src_dir: &Path,
    dst_file: &Path,
    compression_level: i64,
    num_threads: usize,
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
    let worker = CompressionWorker::new(writer, strategy, rx);

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

            // 发送压缩任务
            tx.send(CompressionTask {
                path: path.to_path_buf(),
                name,
                data,
            })?;

            pb.inc(1);
            Ok(())
        }) {
        Ok(_) => {
            // 正常完成
            drop(tx);
            let writer = worker.join()?;
            let mut file = writer.finish()?;
            file.flush()?;
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
// fn progress_bar_init(total_files: Option<u64>) -> anyhow::Result<ProgressBar> {
//     let pb = match total_files {
//         Some(total) => ProgressBar::new(total),
//         None => ProgressBar::new_spinner(),
//     };

//     let style = match total_files {
//         Some(_) => ProgressStyle::default_bar().template(
//             "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
//         )?,
//         None => ProgressStyle::default_spinner()
//             .template("{spinner:.green} 已扫描: {pos} 个文件 [{elapsed_precise}]")?,
//     };

//     pb.set_style(style.progress_chars("#>-"));
//     Ok(pb)
// }

// /// 核心压缩逻辑
// fn parallel_compress(
//     src_dir: &Path,
//     dst_file: &Path,
//     method: zip::CompressionMethod,
//     num_threads: usize,
//     compression_level: u32,
//     small_file_threshold_kb: u64,
// ) -> anyhow::Result<()> {
//     let small_file_threshold = small_file_threshold_kb * 1024;

//     let scan_pb = progress_bar_init(None)?;
//     let (files, total_size) = {
//         let start = Instant::now();

//         let mut total_size = 0;
//         let files: Vec<_> = WalkDir::new(src_dir)
//             .into_iter()
//             .filter_map(|e| e.ok())
//             .filter(|e| e.file_type().is_file())
//             .inspect(|e| {
//                 total_size += e.metadata().map(|m| m.len()).unwrap_or(0);
//                 scan_pb.inc(1); // 更新进度条
//             })
//             .collect();
//         scan_pb.finish_and_clear(); // 完成后清理进度条
//         println!(
//             "[1/4] 扫描完成: 找到 {} 个文件 ({:.2} {}), 耗时 {:?}",
//             files.len(),
//             if total_size >= 1024 * 1024 * 1024 {
//                 total_size as f64 / 1024.0 / 1024.0 / 1024.0
//             } else {
//                 total_size as f64 / 1024.0 / 1024.0
//             },
//             if total_size >= 1024 * 1024 * 1024 {
//                 "GB"
//             } else {
//                 "MB"
//             },
//             start.elapsed()
//         );
//         (files, total_size)
//     };
//     let pb = progress_bar_init(Some(files.len() as u64))?;
//     println!("压缩管道:{}", num_threads);

//     // 创建带缓冲区的ZIP写入器
//     let file = BufWriter::with_capacity(1024 * 1024, File::create(dst_file)?);
//     let zip_writer = zip::ZipWriter::new(file);
//     let zip = Arc::new(RwLock::new(zip_writer));
//     let write_lock_interval = 10; // 每处理10个文件释放一次锁
//                                   // 原子计数器用于进度跟踪
//     let counter = Arc::new(AtomicUsize::new(0));
//     let options = SimpleFileOptions::default()
//         .compression_method(method)
//         .unix_permissions(0o755);
//     // 使用Rayon全局线程池
//     files.par_iter().try_for_each(|entry| {
//         let path = entry.path();
//         let name = path
//             .strip_prefix(src_dir)
//             .with_context(|| format!("路径 {:?} 不是前缀 {:?}", src_dir, path))?
//             .to_str()
//             .map(|s| s.replace("\\", "/"))
//             .ok_or_else(|| anyhow::anyhow!("路径包含无效字符"))?;
//         let name_clone = name.clone();

//         // 修改3：限制锁作用域
//         let result = (|| -> anyhow::Result<()> {
//             // 按间隔获取锁
//             let mut writer = zip.write().map_err(|_| anyhow::anyhow!("锁获取失败"))?;
//             if path.is_file() {
//                 let file = File::open(path)?;
//                 let file_size = file.metadata()?.len();
//                 // 计算文件压缩时间，大小，并进行打印
//                 let start = Instant::now();
//                 let file_size = entry.metadata().map(|m| m.len()).unwrap_or(0);
//                 let use_compression = file_size > small_file_threshold;

//                 // 把压缩的打印出来
//                 // 根据是否为PDF设置不同的压缩选项
//                 let options: zip::write::FileOptions<'_, ()> = if should_compress(path) {
//                     log::warn!("{}: {}---", name, should_compress(path));
//                     FileOptions::default().compression_method(zip::CompressionMethod::Stored)

//                 // 不压缩
//                 } else {
//                     log::info!("{}: {}", name, should_compress(path));

//                     FileOptions::default().compression_method(zip::CompressionMethod::Deflated)

//                     // 压缩
//                 };

//                 // 使用块作用域确保writer及时释放
//                 {
//                     writer.start_file(name, options)?;

//                     if file_size > 10 * 1024 * 1024 {
//                         // 10MB
//                         // 大文件使用流式处理
//                         let mut reader = BufReader::with_capacity(64 * 1024, file); // 64KB 缓冲区
//                         let mut buffer = Vec::with_capacity(64 * 1024);

//                         loop {
//                             buffer.clear();
//                             let bytes_read = reader.read_to_end(&mut buffer)?;
//                             if bytes_read == 0 {
//                                 break;
//                             }
//                             writer.write_all(&buffer)?;
//                         }
//                     } else {
//                         // 小文件直接读取
//                         let mut buffer = Vec::new();
//                         let mut reader = BufReader::new(file);
//                         reader.read_to_end(&mut buffer)?;
//                         writer.write_all(&buffer)?;
//                     }
//                 } // writer 在这里自动释放

//                 let duration = start.elapsed();
//                 if duration.as_millis() > 100 {
//                     println!(
//                         "警告: 文件 {} 压缩时间较长: {} ms",
//                         name_clone,
//                         duration.as_millis()
//                     );
//                 }
//             } else {
//                 // 无锁写入（需要确保线程安全）
//                 let mut writer = zip.write().map_err(|_| anyhow::anyhow!("锁获取失败"))?;
//                 writer.write_all(&std::fs::read(path)?)?;
//             }
//             // 修改4：强制释放内存映射
//             drop(writer);
//             // 原子更新进度
//             let prev = counter.fetch_add(1, Ordering::Relaxed);
//             if prev % 10 == 0 {
//                 pb.set_position(prev as u64);
//             }
//             Ok(())
//         })();

//         result
//     })?;

//     // 修改最后的完成方式
//     let zip_writer = Arc::try_unwrap(zip)
//         .map_err(|_| anyhow::anyhow!("存在未释放的ZIP写入器引用"))?
//         .into_inner()
//         .map_err(|_| anyhow::anyhow!("锁污染错误"))?;

//     let mut file = zip_writer.finish()?;
//     file.flush()?;
//     pb.finish_with_message("完成");
//     Ok(())
// }

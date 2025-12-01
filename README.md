# Sunrise Zip

一个高性能的并行文件压缩工具，使用 Rust 编写，专为快速压缩大型目录而设计。

## 功能特性

- **并行压缩**：支持多线程并行处理文件，显著提高压缩速度
- **智能压缩策略**：根据文件类型自动选择最佳压缩方法
- **大文件优化**：针对大文件使用内存映射技术，减少内存占用
- **进度显示**：实时显示压缩进度和性能统计
- **文件验证**：支持压缩后的文件完整性验证
- **多彩日志**：彩色日志输出，支持控制台和文件记录
- **多种压缩算法**：支持 Stored、Deflated、DeflatedZlib、DeflatedZlibNg、Bzip2、Zstd 等压缩方法

## 安装方法

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/yourusername/sunrise-zip.git
cd sunrise-zip

# 构建发布版本
cargo build --release
```

### 下载预编译二进制文件

从 [Releases](https://github.com/yourusername/sunrise-zip/releases) 页面下载适合您系统的二进制文件。

## 使用方法

### 基本用法

```bash
# 压缩目录
zip <源目录> <目标压缩文件>

# 指定压缩方法
zip <源目录> <目标压缩文件> --compression-method <方法>

# 指定线程数
zip <源目录> <目标压缩文件> --threads <线程数>

# 指定压缩级别
zip <源目录> <目标压缩文件> -l <压缩级别(1-9)>
```

### 压缩方法

- `Stored`：不压缩，直接存储
- `Deflated`：使用 Deflate 算法压缩（默认）
- `DeflatedZlib`：使用 Zlib 的 Deflate 实现
- `DeflatedZlibNg`：使用 Zlib-ng 的 Deflate 实现
- `Bzip2`：使用 Bzip2 算法压缩
- `Zstd`：使用 Zstandard 算法压缩

### 完整参数列表

```
USAGE:
    zip [OPTIONS] <SOURCE> <DESTINATION>

ARGUMENTS:
    <SOURCE>         源目录的路径
    <DESTINATION>    目标压缩文件的路径

OPTIONS:
    -c, --compression-method <COMPRESSION_METHOD>    选择压缩方法 [default: Deflated] [possible values: Stored, Deflated, DeflatedZlib, DeflatedZlibNg, Bzip2, Zstd]
    -h, --help                                       Print help information
    -l, --compression-level <COMPRESSION_LEVEL>      压缩级别 (1-9) [default: 6]
        --small-file-threshold <SMALL_FILE_THRESHOLD> 小文件阈值 (KB) [default: 1024]
    -t, --threads <THREADS>                          压缩线程数 [default: 4]
        --verbose                                     是否显示验证信息
```


### 核心架构

- **多线程压缩管道**：使用生产者-消费者模式，通过通道传递压缩任务
- **智能文件处理**：根据文件大小选择不同的读取策略
- **内存映射**：大文件使用内存映射技术，减少内存复制
- **压缩策略选择**：根据文件扩展名自动选择最佳压缩方法

### 文件类型处理

工具会根据文件类型自动决定是否压缩：

- **不适合压缩的文件类型**（直接存储）：
  - PDF、MP3、MP4、AVI、MKV、MOV、ZIP、RAR、7Z、GZ、BZ2、EXE、DLL、SO、ISO、DMG、DOCX、XLSX、PPTX、RLIB 等

- **适合压缩的文件类型**：
  - TXT、MD、CSV、JSON、XML、YAML、YML、HTML、CSS、JS、LOG、SQL、RS、PY、JAVA、CPP、C、H、INI、CONF、CONFIG、PROPERTIES、DB、SQLITE、BMP、TIFF、GLB 等

- **未知类型**：根据文件大小决定（小于 1MB 的文件不压缩）

## 开发

### 项目结构

```
src/
├── compress/          # 压缩相关模块
│   ├── mod.rs
│   ├── strategy.rs    # 压缩策略
│   └── worker.rs      # 压缩工作线程
├── error.rs           # 错误定义
├── lib.rs             # 库入口
├── log.rs             # 日志配置
├── main.rs            # 主程序
└── utils/             # 工具模块
    ├── file_type.rs   # 文件类型判断
    ├── mod.rs
    ├── progress.rs    # 进度条
    └── verify.rs      # 文件验证
```

### 构建优化

项目使用了多种 Rust 编译优化选项，以获得最佳性能：

```toml
[profile.release]
opt-level = 3     # 使用最高优化级别
lto = true        # 启用链接时优化
codegen-units = 1 # 使用单个代码生成单元以获得更好的优化
panic = "abort"   # 在 panic 时直接中止，减少二进制大小
strip = true      # 自动剥离符号信息
```

## 许可证

[MIT](LICENSE) 或 [Apache-2.0](LICENSE-APACHE)

## 贡献

欢迎提交 Issue 和 Pull Request！请确保您的代码符合项目的代码风格，并添加必要的测试。

## 更新日志

### v0.1.0

- 初始版本发布
- 实现基本的并行压缩功能
- 支持多种压缩算法
- 添加进度显示和文件验证功能
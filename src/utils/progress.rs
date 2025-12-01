use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};

pub fn create_progress_bar(total: Option<u64>) -> Result<ProgressBar> {
    let pb = match total {
        Some(total) => ProgressBar::new(total),
        None => ProgressBar::new_spinner(),
    };

    let style = match total {
        Some(_) => ProgressStyle::default_bar().template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )?,
        None => ProgressStyle::default_spinner()
            .template("{spinner:.green} 已扫描: {pos} 个文件 [{elapsed_precise}]")?,
    };

    pb.set_style(style.progress_chars("#>-"));
    Ok(pb)
}

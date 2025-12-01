use crate::compress::strategy::CompressionStrategy;
use crate::error::ZipError;
use anyhow::Result;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::thread;
use zip::ZipWriter;
pub struct CompressionWorker {
    handle: thread::JoinHandle<Result<ZipWriter<std::fs::File>>>,
}

impl CompressionWorker {
    pub fn new(
        writer: ZipWriter<std::fs::File>,
        strategy: CompressionStrategy,
        rx: Receiver<CompressionTask>,
    ) -> Self {
        let handle = thread::spawn(move || {
            let mut writer = writer;
            while let Ok(task) = rx.recv() {
                let options = strategy.get_options(&task.path);

                writer.start_file(&task.name, options)?;
                writer.write_all(&task.data)?;
            }
            Ok(writer)
        });

        Self { handle }
    }

    pub fn join(self) -> Result<ZipWriter<std::fs::File>> {
        self.handle
            .join()
            .map_err(|e| ZipError::ThreadError(format!("{:?}", e)))?
    }
}

pub struct CompressionTask {
    pub path: PathBuf,
    pub name: String,
    pub data: Vec<u8>,
}

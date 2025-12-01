pub mod strategy;
pub mod worker;

pub use strategy::CompressionStrategy;
pub use worker::{CompressionTask, CompressionWorker};

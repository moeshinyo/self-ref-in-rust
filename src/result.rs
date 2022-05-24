//!
//! 声明Result类型。
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
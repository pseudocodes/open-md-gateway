use log::{info, LevelFilter};

/// 初始化日志系统
pub fn init_logger() {
    // 使用env_logger初始化日志
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .init();

    info!("Logger initialized");
}

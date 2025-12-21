// src/commands/update.rs

use crate::update; // 引用根目录下的 update 模块
use anyhow::Result;

pub async fn handle_update() -> Result<()> {
    // self_update 是同步阻塞的，必须在 spawn_blocking 中运行
    // 否则会阻塞 tokio 运行时
    tokio::task::spawn_blocking(|| {
        update::update_self()
    }).await??;
    
    Ok(())
}
// src/commands/update.rs

use crate::update;
use anyhow::Result;

pub async fn handle_update() -> Result<()> {
    // 因为 self_update 内部使用了阻塞的 reqwest，
    // 在 tokio 运行时中直接调用可能会报错，我们需要用 spawn_blocking
    tokio::task::spawn_blocking(|| {
        update::update_self()
    }).await??;
    
    Ok(())
}
// src/commands/ip.rs

use crate::{api, config, utils};
use crate::utils::Target;
use anyhow::{Context, Result};
use colored::Colorize;
use std::net::ToSocketAddrs;

/// Resolve IP address for a target
/// Supports both Node ID (e.g., "12345") and App target (e.g., "api.RedQ")
pub async fn handle_ip(target_str: String) -> Result<()> {
    let target = utils::parse_target(&target_str)?;
    let full_domain = target.domain();

    // Node 目标优先走 API: 除了公网 IP 还能拿到心跳上报的内网 IP。
    // 未登录或 API 失败时回落到下面的 DNS 解析。
    if let Target::NodeId { id, .. } = &target {
        if let Some(token) = config::load_config().ok().and_then(|c| c.token) {
            if let Ok(node) = api::get_node(&token, *id).await {
                println!("{}", node.ip_address.green());
                if let Some(lan) = &node.lan_ip {
                    println!("{} {}", lan.cyan(), "(LAN)".dimmed());
                }
                return Ok(());
            }
        }
    }

    o_step!("Resolving IP for {}...", full_domain.cyan());

    // 使用标准库进行 DNS 查询
    // (domain, 0) 是一个技巧，表示任何端口
    let addrs_iter = (full_domain.as_str(), 0).to_socket_addrs()
        .with_context(|| format!("Failed to resolve DNS for '{}'", full_domain))?;

    let mut found_ip = false;
    for addr in addrs_iter {
        // 我们只关心 IPv4 地址，因为这是后端设置的
        if let std::net::SocketAddr::V4(socket_addr_v4) = addr {
            println!("{}", socket_addr_v4.ip().to_string().green());
            found_ip = true;
            break; // 找到第一个就退出
        }
    }

    if !found_ip {
        return Err(anyhow::anyhow!("Could not find an IPv4 address for the specified target."));
    }

    Ok(())
}
// src/commands/caddy.rs
//
// 节点上重载 Caddy —— 所有命令只用这一种办法。
//
// 2026-09-28(mafold api@0.0.162):`systemctl reload caddy` 永远没返回。Caddy 其实已经换上了
// 新配置,但旧的 admin endpoint 关不掉(`stopping admin server: 10s timeout`),于是 Caddy 始终
// 不给 systemd 发 READY=1;systemd 停在 `reload-notify`,每 90 秒杀一次 ExecReload,却从不结束
// 这个 reload job —— 跑它的 ssh、跑 ssh 的 CI 一起挂了 30 多分钟。同样的报错在 09-22 以来每次
// api 发版里都有(那几次 ~4.5 分钟后变红),连续 7 次发版没出 GitHub Release,没人发现。
//
// 所以 reload 不再问 systemd:直接让 Caddy 重载、给它一个时限,然后**看**结果 —— Caddy 正在跑
// 的配置(admin `/config/`)必须等于 Caddyfile 翻译出来的配置。重载报了错但其实已经生效(这次
// 就是这样)= 成功;重载返回 0 但没生效 = 失败。

use crate::commands::ssh::SshSession;
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::time::Duration;

pub const CADDYFILE: &str = "/etc/caddy/Caddyfile";

/// `caddy reload` 在节点上最多跑这么久(远端 `timeout` 杀它)。正常 1 秒内完成。
pub const RELOAD_DEADLINE_SECS: u64 = 60;

/// 核对最多等这么多轮、每轮隔几秒 —— 一次还在收尾的加载可能晚一点才在 `/config/` 里看得到。
const VERIFY_ROUNDS: u32 = 6;
const VERIFY_EVERY: Duration = Duration::from_secs(5);

/// reload 之后,被代理的长连接(WebSocket)再留多久才由 Caddy 关掉。
///
/// 2026-09-28 查实:Caddy 卸载旧配置时,默认(`stream_close_delay 0`)会**同步**给每条
/// WebSocket 写一个关闭帧,不设写超时(`reverseproxy/streaming.go` 的 `closeConnections`,
/// 源码注释自己写着 "potentially blocking")。只要有一个客户端的 TCP 不再收数据(机器睡了、
/// 网络半断),这一写就挂到内核判它超时为止 —— reload 跟着挂,旧 admin 关不掉(`10s timeout`),
/// `/config/` 也被锁住。node11 上 7 次都是这样,最长一次 52 分钟。设了这个值,关闭交给后台
/// 定时器,reload 当场返回;代价只是旧连接多活这么久,之后照常关、客户端照常重连。
pub const STREAM_CLOSE_DELAY: &str = "10s";

/// 每条生成出来的路由都用这一个 `reverse_proxy` —— 选项只写在这里,别处不许手拼。
/// 返回值放在 4 格缩进的 `handle { … }` 里。
pub fn reverse_proxy(upstream: &str) -> String {
    format!("reverse_proxy {upstream} {{\n        stream_close_delay {STREAM_CLOSE_DELAY}\n    }}")
}

/// 远端要跑的重载命令本身:先校验,再直接(不经 systemd)带时限地让 Caddy 重载。
/// 没有 `SshSession` 的调用方(`tunnel.rs`)用它;有的用 `reload_and_verify`。
pub fn reload_script() -> String {
    format!(
        "caddy validate --config {CADDYFILE} && timeout {RELOAD_DEADLINE_SECS} caddy reload --config {CADDYFILE} --force"
    )
}

/// Caddy 正在跑的配置是不是 Caddyfile 翻译出来的那份。按 JSON 值比,空白和键的顺序不算差别。
pub fn running_matches(intended: &[u8], running: &[u8]) -> Result<bool> {
    let a: serde_json::Value = serde_json::from_slice(intended).context("`caddy adapt` did not print JSON")?;
    let b: serde_json::Value = serde_json::from_slice(running).context("admin `/config/` did not return JSON")?;
    Ok(a == b)
}

/// 校验 → 直接重载(有时限)→ 从 admin 接口核对新配置真的在跑。任何一步挂住都有上限。
pub fn reload_and_verify(session: &SshSession) -> Result<()> {
    session
        .exec_timeout(&format!("caddy validate --config {CADDYFILE}"), None, Duration::from_secs(60))
        .context("Caddyfile does not validate")?;

    let reload = session.exec_timeout(
        &format!("timeout {RELOAD_DEADLINE_SECS} caddy reload --config {CADDYFILE} --force"),
        None,
        Duration::from_secs(RELOAD_DEADLINE_SECS + 30),
    );

    let mut last = String::new();
    for round in 0..VERIFY_ROUNDS {
        if round > 0 {
            std::thread::sleep(VERIFY_EVERY);
        }
        let intended = session.exec_output_timeout(
            &format!("caddy adapt --config {CADDYFILE} 2>/dev/null"),
            Duration::from_secs(30),
        );
        let running = session.exec_output_timeout(
            "curl -fsS --max-time 10 http://localhost:2019/config/",
            Duration::from_secs(30),
        );
        match (intended, running) {
            (Ok(i), Ok(r)) => match running_matches(&i, &r) {
                Ok(true) => {
                    if let Err(e) = &reload {
                        crate::o_warn!(
                            "   {} caddy reload reported an error ({:#}), but the running config is the new one",
                            "⚠".yellow(),
                            e
                        );
                    }
                    return Ok(());
                }
                Ok(false) => last = "the running config is not the one the Caddyfile adapts to".into(),
                Err(e) => last = format!("{e:#}"),
            },
            (Err(e), _) | (_, Err(e)) => last = format!("{e:#}"),
        }
    }
    Err(anyhow!(
        "Caddy is not running the new config ({last}); the reload {}",
        match &reload {
            Ok(()) => "exited 0".to_string(),
            Err(e) => format!("said: {e:#}"),
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 一个不收数据的 WebSocket 客户端不能再把 reload 挂住:每条代理都要把关闭交给后台。
    #[test]
    fn every_generated_proxy_hands_stream_closing_to_a_timer() {
        let block = reverse_proxy("127.0.0.1:4000");
        assert!(block.starts_with("reverse_proxy 127.0.0.1:4000 {"), "{block}");
        assert!(block.contains(&format!("stream_close_delay {STREAM_CLOSE_DELAY}")), "{block}");
        assert!(block.trim_end().ends_with('}'), "{block}");
    }

    /// 生成路由的地方只准调 `reverse_proxy()`:谁手拼一个,就又回到默认的同步关闭。
    #[test]
    fn no_route_generator_spells_reverse_proxy_by_hand() {
        for (file, src) in [
            ("deploy.rs", include_str!("deploy.rs")),
            ("tunnel.rs", include_str!("tunnel.rs")),
            ("init.rs", include_str!("init.rs")),
        ] {
            let hand_rolled: Vec<&str> = src
                .lines()
                .filter(|l| l.contains("reverse_proxy ") && !l.trim_start().starts_with("//"))
                .collect();
            assert!(hand_rolled.is_empty(), "{file} builds its own reverse_proxy: {hand_rolled:#?}");
        }
    }

    #[test]
    fn a_reload_never_goes_through_systemd_and_has_a_deadline() {
        let s = reload_script();
        assert!(!s.contains("systemctl"), "{s}");
        assert!(s.contains(&format!("timeout {RELOAD_DEADLINE_SECS} caddy reload")), "{s}");
        assert!(s.starts_with("caddy validate"), "validate first: {s}");
    }

    #[test]
    fn the_same_config_counts_whatever_its_formatting() {
        let adapted = br#"{"apps":{"http":{"grace_period":30000000000,"servers":{"srv0":{"listen":[":80"]}}}}}"#;
        let running = b"{\n  \"apps\": { \"http\": { \"servers\": { \"srv0\": { \"listen\": [\":80\"] } }, \"grace_period\": 30000000000 } }\n}";
        assert!(running_matches(adapted, running).unwrap());
    }

    #[test]
    fn a_running_config_without_the_new_route_is_not_a_reload() {
        let adapted = br#"{"apps":{"http":{"servers":{"srv0":{"routes":[{"match":[{"host":["api.mafold.com"]}]}]}}}}}"#;
        let running = br#"{"apps":{"http":{"servers":{"srv0":{"routes":[]}}}}}"#;
        assert!(!running_matches(adapted, running).unwrap());
    }

    #[test]
    fn garbage_from_either_side_is_an_error_not_a_pass() {
        assert!(running_matches(b"not json", b"{}").is_err());
        assert!(running_matches(b"{}", b"<html>502</html>").is_err());
    }
}

// src/session.rs
//! The saved login on this machine: read its claims and keep it alive.
//!
//! Since 2026-10 a login token lasts 30 days and carries the account's token
//! version (`ver`). Before each command the CLI renews a token that has less
//! than a week left, so a machine in use never gets logged out; one left idle
//! past expiry has to `ops login` again. `ops token revoke-all` bumps the
//! version, which kills every earlier token of the account at once.

use crate::{api, config};
use std::time::{SystemTime, UNIX_EPOCH};

/// Renew when less than this is left.
const RENEW_WITHIN_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Default, PartialEq)]
pub struct Claims {
    pub exp: Option<i64>,
    pub iat: Option<i64>,
    pub ver: Option<i64>,
    pub name: Option<String>,
}

/// Decode a JWT's payload WITHOUT verifying it —— only to decide locally
/// whether it is time to renew. The server is the one that verifies.
pub fn peek_claims(token: &str) -> Option<Claims> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some(Claims {
        exp: v.get("exp").and_then(|x| x.as_i64()),
        iat: v.get("iat").and_then(|x| x.as_i64()),
        ver: v.get("ver").and_then(|x| x.as_i64()),
        name: v.get("name").and_then(|x| x.as_str()).map(str::to_string),
    })
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            _ => None,
        }
    }
    let input: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    for chunk in input.chunks(4) {
        if chunk.len() == 1 {
            return None;
        }
        let mut acc: u32 = 0;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i as u32);
        }
        out.push((acc >> 16) as u8);
        if chunk.len() > 2 {
            out.push((acc >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// True when a token expiring at `exp` should be renewed at `now`.
pub fn needs_renewal(exp: i64, now: i64) -> bool {
    exp > now && exp - now <= RENEW_WITHIN_SECS
}

/// Renew the saved login if it is about to expire. Best effort: any failure is
/// left for the command itself to report (an expired or revoked token gets a
/// clear 401 from the server).
///
/// A token passed in `OPS_TOKEN` (CI) is never touched —— there is nowhere to
/// write the new one back; CI tokens are issued for up to a year instead
/// (`ops token create`).
pub async fn refresh_if_expiring() {
    if std::env::var("OPS_TOKEN").map(|t| !t.is_empty()).unwrap_or(false) {
        return;
    }
    let Ok(cfg) = config::load_config() else { return };
    let Some(token) = cfg.token else { return };
    let Some(exp) = peek_claims(&token).and_then(|c| c.exp) else { return };
    if !needs_renewal(exp, unix_now()) {
        return;
    }
    // stderr, not o_debug!: stdout of `ops token` / `ops token create` is the token itself
    let verbose = crate::output::verbosity() >= crate::output::Verbosity::Verbose;
    match api::refresh_token(&token).await {
        Ok(res) => {
            if config::save_config(&config::Config { token: Some(res.token) }).is_ok() && verbose {
                eprintln!("Login renewed (now valid until {})", res.expires_at.unwrap_or_default());
            }
        }
        Err(e) if verbose => eprintln!("Login renewal failed: {}", e),
        Err(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // {"userId":2,"username":"ops","ver":3,"iat":1791700000,"exp":1794292000,"name":"ci"}
    const TOKEN: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VySWQiOjIsInVzZXJuYW1lIjoib3BzIiwidmVyIjozLCJpYXQiOjE3OTE3MDAwMDAsImV4cCI6MTc5NDI5MjAwMCwibmFtZSI6ImNpIn0.sig";
    // {"userId":2,"username":"ops","exp":4931000000}  —— pre-2026-10 format
    const LEGACY: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJ1c2VySWQiOjIsInVzZXJuYW1lIjoib3BzIiwiZXhwIjo0OTMxMDAwMDAwfQ.sig";

    #[test]
    fn reads_claims_of_a_versioned_token() {
        assert_eq!(
            peek_claims(TOKEN),
            Some(Claims { exp: Some(1794292000), iat: Some(1791700000), ver: Some(3), name: Some("ci".into()) })
        );
    }

    #[test]
    fn reads_claims_of_a_legacy_token() {
        let c = peek_claims(LEGACY).unwrap();
        assert_eq!(c.exp, Some(4931000000));
        assert_eq!(c.ver, None);
        assert_eq!(c.iat, None);
    }

    #[test]
    fn garbage_is_not_a_token() {
        assert_eq!(peek_claims("not-a-jwt"), None);
        assert_eq!(peek_claims("a.%%%.c"), None);
    }

    #[test]
    fn renews_only_inside_the_last_week() {
        let now = 1_000_000_000;
        let day = 86_400;
        assert!(!needs_renewal(now + 30 * day, now), "fresh 30-day token");
        assert!(!needs_renewal(now + 8 * day, now));
        assert!(needs_renewal(now + 7 * day, now));
        assert!(needs_renewal(now + 60, now));
        assert!(!needs_renewal(now - 1, now), "already expired: the server says so, renewing can't help");
        assert!(!needs_renewal(now + 100 * 365 * day, now), "legacy 100-year token");
    }
}

use anyhow::Result;
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct SystemMetrics {
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
    pub uptime_seconds: u64,
    pub load_average: [f64; 3],
}

pub fn collect_metrics() -> Result<SystemMetrics> {
    let cpu_percent = read_cpu_percent().unwrap_or(0.0);
    let (mem_used, mem_total) = read_memory().unwrap_or((0, 0));
    let (disk_used, disk_total) = read_disk().unwrap_or((0.0, 0.0));
    let uptime = read_uptime().unwrap_or(0);
    let load = read_loadavg().unwrap_or([0.0, 0.0, 0.0]);

    Ok(SystemMetrics {
        cpu_percent,
        memory_used_mb: mem_used,
        memory_total_mb: mem_total,
        disk_used_gb: disk_used,
        disk_total_gb: disk_total,
        uptime_seconds: uptime,
        load_average: load,
    })
}

fn read_cpu_percent() -> Result<f64> {
    // Read /proc/stat twice with a small delay to compute CPU usage
    let stat1 = std::fs::read_to_string("/proc/stat")?;
    std::thread::sleep(std::time::Duration::from_millis(250));
    let stat2 = std::fs::read_to_string("/proc/stat")?;

    let parse_cpu_line = |line: &str| -> Option<(u64, u64)> {
        let parts: Vec<u64> = line.split_whitespace().skip(1).filter_map(|s| s.parse().ok()).collect();
        if parts.len() < 4 {
            return None;
        }
        let idle = parts[3];
        let total: u64 = parts.iter().sum();
        Some((idle, total))
    };

    let line1 = stat1.lines().next().unwrap_or("");
    let line2 = stat2.lines().next().unwrap_or("");

    let (idle1, total1) = parse_cpu_line(line1).unwrap_or((0, 1));
    let (idle2, total2) = parse_cpu_line(line2).unwrap_or((0, 1));

    let idle_delta = idle2.saturating_sub(idle1) as f64;
    let total_delta = total2.saturating_sub(total1) as f64;

    if total_delta == 0.0 {
        return Ok(0.0);
    }

    Ok(((total_delta - idle_delta) / total_delta * 100.0 * 10.0).round() / 10.0)
}

fn read_memory() -> Result<(u64, u64)> {
    let meminfo = std::fs::read_to_string("/proc/meminfo")?;
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;

    for line in meminfo.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = parse_meminfo_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = parse_meminfo_value(line);
        }
    }

    let total_mb = total_kb / 1024;
    let used_mb = total_mb - (available_kb / 1024);
    Ok((used_mb, total_mb))
}

fn parse_meminfo_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn read_disk() -> Result<(f64, f64)> {
    let stat = nix::sys::statvfs::statvfs("/")?;
    let total_bytes = stat.blocks() as u64 * stat.fragment_size() as u64;
    let free_bytes = stat.blocks_available() as u64 * stat.fragment_size() as u64;
    let used_bytes = total_bytes - free_bytes;

    let to_gb = |b: u64| (b as f64 / 1_073_741_824.0 * 10.0).round() / 10.0;
    Ok((to_gb(used_bytes), to_gb(total_bytes)))
}

fn read_uptime() -> Result<u64> {
    let content = std::fs::read_to_string("/proc/uptime")?;
    let seconds: f64 = content
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()?;
    Ok(seconds as u64)
}

fn read_loadavg() -> Result<[f64; 3]> {
    let content = std::fs::read_to_string("/proc/loadavg")?;
    let parts: Vec<f64> = content
        .split_whitespace()
        .take(3)
        .filter_map(|s| s.parse().ok())
        .collect();

    Ok([
        *parts.first().unwrap_or(&0.0),
        *parts.get(1).unwrap_or(&0.0),
        *parts.get(2).unwrap_or(&0.0),
    ])
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum AutoTunerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to detect system information: {0}")]
    DetectionFailed(String),
}

pub type Result<T> = std::result::Result<T, AutoTunerError>;

// ─── System information ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub cpu_cores: u32,
    pub cpu_name: String,
    pub os: String,
    pub arch: String,
    pub java_major: u32,
}

// ─── JVM recommendation ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct JvmRecommendation {
    pub max_memory: String,
    pub min_memory: String,
    pub thread_stack_size: String,
    pub gc_type: GcType,
    pub extra_flags: Vec<String>,
    pub reasoning: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GcType {
    Zgc,
    G1gc,
    Serial,
}

impl std::fmt::Display for GcType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zgc => write!(f, "ZGC"),
            Self::G1gc => write!(f, "G1GC"),
            Self::Serial => write!(f, "Serial"),
        }
    }
}

// ─── Memory chart data ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MemoryChartEntry {
    pub label: String,
    pub mb: u64,
}

// ─── Detection ─────────────────────────────────────────────────────

pub fn detect_system() -> Result<SystemInfo> {
    let total_ram_mb = detect_total_ram()?;
    let available_ram_mb = detect_available_ram().unwrap_or(total_ram_mb);
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    let cpu_name = detect_cpu_name();
    let java_major = detect_java_major().unwrap_or(21);

    Ok(SystemInfo {
        total_ram_mb,
        available_ram_mb,
        cpu_cores,
        cpu_name,
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        java_major,
    })
}

fn detect_total_ram() -> Result<u64> {
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo")?;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                let kb: u64 = rest
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                return Ok(kb / 1024);
            }
        }
        return Err(AutoTunerError::DetectionFailed(
            "Could not parse /proc/meminfo".into(),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()?;
        let bytes: u64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .unwrap_or(0);
        return Ok(bytes / (1024 * 1024));
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, fall back to a reasonable default — the real launcher
        // will use sysinfo or WMI when we add that dependency
        return Ok(8192);
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Ok(8192)
    }
}

fn detect_available_ram() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        for line in meminfo.lines() {
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
                return Some(kb / 1024);
            }
        }
    }
    None
}

fn detect_cpu_name() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if let Some(name) = line.strip_prefix("model name") {
                    return name.trim().trim_start_matches(':').trim().to_string();
                }
            }
        }
    }
    format!("{} {}", std::env::consts::ARCH, "CPU")
}

fn detect_java_major() -> Option<u32> {
    let output = std::process::Command::new("java").arg("-version").output().ok()?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().next()?;
    let version_str = line.split('"').nth(1)?;
    let major_str = version_str.split('.').next()?;
    if major_str == "1" {
        let minor = version_str.split('.').nth(1)?;
        return minor.parse().ok();
    }
    major_str.parse().ok()
}

// ─── Recommendation engine ─────────────────────────────────────────

pub fn recommend(mc_version: &str, mod_count: usize, system: &SystemInfo) -> JvmRecommendation {
    let base_mb = base_memory(mc_version);
    let overhead_mb = mod_overhead(mod_count);
    let requested_mb = base_mb + overhead_mb;

    let cap_75 = (system.total_ram_mb * 75) / 100;
    let cap_max = 12 * 1024;
    let raw = requested_mb.min(cap_75).min(cap_max);
    let max_mb = round_to_512(raw.max(1024));
    let max_label = format_mb(max_mb);

    let gc_type = select_gc(system.java_major, mc_version);
    let extra_flags = build_extra_flags(&gc_type, system, mod_count);
    let reasoning = build_reasoning(mc_version, mod_count, system, max_mb, &gc_type);

    JvmRecommendation {
        max_memory: max_label.clone(),
        min_memory: max_label,
        thread_stack_size: "1M".to_string(),
        gc_type,
        extra_flags,
        reasoning,
    }
}

fn base_memory(mc_version: &str) -> u64 {
    let minor = mc_version
        .split('.')
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    match minor {
        8..=16 => 2 * 1024,
        17..=19 => 3 * 1024,
        _ => 4 * 1024,
    }
}

fn mod_overhead(mod_count: usize) -> u64 {
    match mod_count {
        0 => 0,
        1..=20 => 1024,
        21..=50 => 2048,
        51..=100 => 3072,
        101..=200 => 4096,
        _ => 6144,
    }
}

fn round_to_512(mb: u64) -> u64 {
    ((mb + 256) / 512) * 512
}

fn format_mb(mb: u64) -> String {
    if mb >= 1024 {
        let gb = mb as f64 / 1024.0;
        if (gb * 2.0).fract() < 0.01 {
            format!("{}G", gb as u64)
        } else {
            format!("{:.1}G", gb)
        }
    } else {
        format!("{}M", mb)
    }
}

fn select_gc(java_major: u32, mc_version: &str) -> GcType {
    let minor = mc_version
        .split('.')
        .nth(1)
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    if java_major >= 21 && minor >= 20 {
        GcType::Zgc
    } else if java_major >= 9 {
        GcType::G1gc
    } else {
        GcType::Serial
    }
}

fn build_extra_flags(gc: &GcType, system: &SystemInfo, mod_count: usize) -> Vec<String> {
    let mut flags = Vec::new();

    flags.push("-Dfml.ignorePatchDiscrepancies=true".to_string());

    match gc {
        GcType::Zgc => {
            flags.push("-XX:+UseZGC".to_string());
            flags.push("-XX:+ZGenerational".to_string());
        }
        GcType::G1gc => {
            flags.push("-XX:+UseG1GC".to_string());
            flags.push("-XX:+UnlockExperimentalVMOptions".to_string());
            flags.push("-XX:G1HeapRegionSize=16M".to_string());
        }
        GcType::Serial => {}
    }

    if mod_count > 50 {
        flags.push("-XX:+UseStringDeduplication".to_string());
    }

    if system.java_major >= 12 {
        flags.push("-XX:+AlwaysPreTouch".to_string());
    }

    if system.total_ram_mb > 8 * 1024 {
        let threads = system.cpu_cores.min(8);
        flags.push(format!("-XX:ParallelGCThreads={}", threads));
    }

    flags
}

fn build_reasoning(
    mc_version: &str,
    mod_count: usize,
    system: &SystemInfo,
    max_mb: u64,
    gc: &GcType,
) -> String {
    let total_gb = system.total_ram_mb as f64 / 1024.0;
    let alloc_gb = max_mb as f64 / 1024.0;

    let mut parts = vec![format!(
        "Recommended {:.0}G RAM for {} with {} mods on {:.0}G system",
        alloc_gb, mc_version, mod_count, total_gb,
    )];

    parts.push(format!("Using {} garbage collector", gc));

    if system.total_ram_mb < 4 * 1024 {
        parts
            .push("Warning: system has less than 4GB RAM — Minecraft may struggle".into());
    }

    if mod_count > 100 {
        parts.push(format!(
            "Heavy modpack (+{}MB overhead) — consider reducing render distance in-game",
            mod_overhead(mod_count) / 1024
        ));
    }

    parts.join(". ")
}

// ─── Memory chart ──────────────────────────────────────────────────

pub fn get_memory_chart(system: &SystemInfo, recommendation: &JvmRecommendation) -> Vec<MemoryChartEntry> {
    let alloc_mb = parse_memory_label(&recommendation.max_memory);
    let os_reserved = system.total_ram_mb / 4;

    vec![
        MemoryChartEntry {
            label: "System Total".into(),
            mb: system.total_ram_mb,
        },
        MemoryChartEntry {
            label: "Recommended".into(),
            mb: alloc_mb,
        },
        MemoryChartEntry {
            label: "OS Reserved".into(),
            mb: os_reserved,
        },
        MemoryChartEntry {
            label: "Available".into(),
            mb: system.total_ram_mb.saturating_sub(os_reserved),
        },
    ]
}

fn parse_memory_label(label: &str) -> u64 {
    if let Some(gb_str) = label.strip_suffix('G') {
        let gb: f64 = gb_str.parse().unwrap_or(0.0);
        (gb * 1024.0) as u64
    } else if let Some(mb_str) = label.strip_suffix('M') {
        mb_str.parse().unwrap_or(0)
    } else {
        4096
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_system(total_ram_mb: u64, cpu_cores: u32, java_major: u32) -> SystemInfo {
        SystemInfo {
            total_ram_mb,
            available_ram_mb: total_ram_mb,
            cpu_cores,
            cpu_name: "Test CPU".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            java_major,
        }
    }

    #[test]
    fn test_base_memory() {
        assert_eq!(base_memory("1.8.9"), 2048);
        assert_eq!(base_memory("1.16.5"), 2048);
        assert_eq!(base_memory("1.17.1"), 3072);
        assert_eq!(base_memory("1.20.4"), 4096);
        assert_eq!(base_memory("1.21.4"), 4096);
    }

    #[test]
    fn test_mod_overhead() {
        assert_eq!(mod_overhead(0), 0);
        assert_eq!(mod_overhead(10), 1024);
        assert_eq!(mod_overhead(30), 2048);
        assert_eq!(mod_overhead(75), 3072);
        assert_eq!(mod_overhead(150), 4096);
        assert_eq!(mod_overhead(300), 6144);
    }

    #[test]
    fn test_round_to_512() {
        assert_eq!(round_to_512(1024), 1024);
        assert_eq!(round_to_512(1200), 1536);
        assert_eq!(round_to_512(2600), 3072);
        assert_eq!(round_to_512(4096), 4096);
    }

    #[test]
    fn test_select_gc() {
        assert_eq!(select_gc(21, "1.21.4"), GcType::Zgc);
        assert_eq!(select_gc(17, "1.20.4"), GcType::G1gc);
        assert_eq!(select_gc(8, "1.16.5"), GcType::Serial);
        assert_eq!(select_gc(21, "1.16.5"), GcType::G1gc);
    }

    #[test]
    fn test_recommend_capped_at_75_percent() {
        let sys = fake_system(4096, 4, 17);
        let rec = recommend("1.20.4", 0, &sys);
        let mb = parse_memory_label(&rec.max_memory);
        assert!(mb <= 4096 * 75 / 100);
    }

    #[test]
    fn test_recommend_capped_at_12gb() {
        let sys = fake_system(64 * 1024, 16, 21);
        let rec = recommend("1.21.4", 300, &sys);
        let mb = parse_memory_label(&rec.max_memory);
        assert!(mb <= 12 * 1024);
    }

    #[test]
    fn test_recommend_minimum_1gb() {
        let sys = fake_system(2048, 2, 8);
        let rec = recommend("1.8.9", 0, &sys);
        let mb = parse_memory_label(&rec.max_memory);
        assert!(mb >= 1024);
    }

    #[test]
    fn test_memory_chart_length() {
        let sys = fake_system(16384, 8, 21);
        let rec = recommend("1.21.4", 10, &sys);
        let chart = get_memory_chart(&sys, &rec);
        assert_eq!(chart.len(), 4);
        assert_eq!(chart[0].label, "System Total");
    }

    #[test]
    fn test_format_mb() {
        assert_eq!(format_mb(1024), "1G");
        assert_eq!(format_mb(2048), "2G");
        assert_eq!(format_mb(3072), "3G");
        assert_eq!(format_mb(4096), "4G");
    }

    #[test]
    fn test_parse_memory_label() {
        assert_eq!(parse_memory_label("4G"), 4096);
        assert_eq!(parse_memory_label("2G"), 2048);
        assert_eq!(parse_memory_label("512M"), 512);
    }
}

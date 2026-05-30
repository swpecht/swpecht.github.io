//! Lightweight diagnostics helpers — currently just process memory
//! sampling for kestrel metric streams.

use std::fs;

/// Snapshot of the calling process's memory footprint, in kilobytes.
///
/// All three fields use the kernel's `/proc/self/status` units (kB),
/// matching what `top`/`htop` show. Convert to MB at the call site.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcMem {
    /// Current resident set size — the actually-in-RAM portion of the
    /// process. This is the number that matters for OOM.
    pub rss_kb: u64,
    /// Peak resident high-water mark (`VmHWM` on Linux). Catches
    /// transient spikes between report cycles.
    pub peak_rss_kb: u64,
    /// Total virtual address space (`VmSize`). Usually larger than RSS;
    /// useful as a leading indicator on overcommit-prone systems.
    pub vsize_kb: u64,
}

impl ProcMem {
    pub fn rss_mb(&self) -> f64 {
        self.rss_kb as f64 / 1024.0
    }
    pub fn peak_rss_mb(&self) -> f64 {
        self.peak_rss_kb as f64 / 1024.0
    }
    pub fn vsize_mb(&self) -> f64 {
        self.vsize_kb as f64 / 1024.0
    }
}

/// Sample the current process's memory footprint.
///
/// Tries `/proc/self/status` (Linux). Returns `None` on platforms or
/// configurations where that file isn't readable. Caller decides how to
/// emit the metric on a missing sample (e.g. emit `-1` so the line is
/// still parseable).
pub fn process_memory() -> Option<ProcMem> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let mut out = ProcMem::default();
    for line in status.lines() {
        let mut parts = line.split_whitespace();
        let key = parts.next()?;
        let val = match parts.next() {
            Some(v) => v,
            None => continue,
        };
        let n: u64 = match val.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        match key {
            "VmRSS:" => out.rss_kb = n,
            "VmHWM:" => out.peak_rss_kb = n,
            "VmSize:" => out.vsize_kb = n,
            _ => {}
        }
    }
    // If we read the file but found none of the expected fields,
    // treat that as a failed sample.
    if out.rss_kb == 0 && out.peak_rss_kb == 0 && out.vsize_kb == 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// We can't assert exact values, but on a Linux test runner the
    /// helper should at least return *some* positive RSS for our own
    /// process.
    #[test]
    fn process_memory_returns_positive_rss_on_linux() {
        let Some(mem) = process_memory() else {
            // /proc/self/status unavailable (e.g. macOS in dev). Skip.
            return;
        };
        assert!(mem.rss_kb > 0, "RSS should be positive, got {}", mem.rss_kb);
        // peak_rss >= rss always.
        assert!(
            mem.peak_rss_kb >= mem.rss_kb,
            "peak_rss ({}) should be >= rss ({})",
            mem.peak_rss_kb,
            mem.rss_kb,
        );
        // vsize >= rss as well.
        assert!(
            mem.vsize_kb >= mem.rss_kb,
            "vsize ({}) should be >= rss ({})",
            mem.vsize_kb,
            mem.rss_kb,
        );
    }
}

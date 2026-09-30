//! Resource limits and usage tracking

use serde::{Deserialize, Serialize};

/// Resource limits for a sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum CPU usage percentage (0-100)
    pub max_cpu_percent: f32,
    /// Maximum memory usage in MB
    pub max_memory_mb: u64,
    /// Maximum disk read in MB
    pub max_disk_read_mb: u64,
    /// Maximum disk write in MB
    pub max_disk_write_mb: u64,
    /// Maximum network bandwidth in KB/s
    pub max_network_kbps: u64,
    /// Maximum number of processes
    pub max_processes: u32,
    /// Maximum execution time in seconds per task
    pub max_task_duration_secs: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 50.0,
            max_memory_mb: 512,
            max_disk_read_mb: 1024,
            max_disk_write_mb: 1024,
            max_network_kbps: 1024,
            max_processes: 10,
            max_task_duration_secs: 300,
        }
    }
}

impl ResourceLimits {
    /// Create unrestricted limits (for host execution)
    pub fn unlimited() -> Self {
        Self {
            max_cpu_percent: 100.0,
            max_memory_mb: u64::MAX,
            max_disk_read_mb: u64::MAX,
            max_disk_write_mb: u64::MAX,
            max_network_kbps: u64::MAX,
            max_processes: u32::MAX,
            max_task_duration_secs: u64::MAX,
        }
    }

    /// Create conservative limits (for untrusted code)
    pub fn conservative() -> Self {
        Self {
            max_cpu_percent: 25.0,
            max_memory_mb: 256,
            max_disk_read_mb: 512,
            max_disk_write_mb: 256,
            max_network_kbps: 512,
            max_processes: 5,
            max_task_duration_secs: 60,
        }
    }
}

/// Current resource usage
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Current CPU usage percentage
    pub cpu_percent: f32,
    /// Current memory usage in MB
    pub memory_mb: u64,
    /// Total disk read in MB
    pub disk_read_mb: u64,
    /// Total disk write in MB
    pub disk_write_mb: u64,
    /// Network bytes sent
    pub network_bytes_sent: u64,
    /// Network bytes received
    pub network_bytes_received: u64,
    /// Number of active processes
    pub process_count: u32,
    /// Task execution time in seconds
    pub task_duration_secs: u64,
}

impl ResourceUsage {
    /// Check if any limit is exceeded
    pub fn exceeds(&self, limits: &ResourceLimits) -> Vec<String> {
        let mut violations = Vec::new();

        if self.cpu_percent > limits.max_cpu_percent {
            violations.push(format!(
                "CPU: {:.1}% > {}%",
                self.cpu_percent, limits.max_cpu_percent
            ));
        }

        if self.memory_mb > limits.max_memory_mb {
            violations.push(format!(
                "Memory: {}MB > {}MB",
                self.memory_mb, limits.max_memory_mb
            ));
        }

        if self.disk_read_mb > limits.max_disk_read_mb {
            violations.push(format!(
                "Disk read: {}MB > {}MB",
                self.disk_read_mb, limits.max_disk_read_mb
            ));
        }

        if self.disk_write_mb > limits.max_disk_write_mb {
            violations.push(format!(
                "Disk write: {}MB > {}MB",
                self.disk_write_mb, limits.max_disk_write_mb
            ));
        }

        if self.process_count > limits.max_processes {
            violations.push(format!(
                "Processes: {} > {}",
                self.process_count, limits.max_processes
            ));
        }

        if self.task_duration_secs > limits.max_task_duration_secs {
            violations.push(format!(
                "Duration: {}s > {}s",
                self.task_duration_secs, limits.max_task_duration_secs
            ));
        }

        violations
    }
}

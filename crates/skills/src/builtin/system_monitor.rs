//! System monitor — CPU, memory, disk, network, process info

use async_trait::async_trait;
use ravenbot_core::Permission;
use crate::traits::{Skill, SkillContext, SkillError, SkillResult};

pub struct SystemMonitorSkill;

impl SystemMonitorSkill { pub fn new() -> Self { Self } }

#[async_trait]
impl Skill for SystemMonitorSkill {
    fn id(&self) -> &str { "system_monitor" }
    fn name(&self) -> &str { "System Monitor" }
    fn description(&self) -> &str { "Get system resource usage: CPU, memory, disk, network, and running processes" }
    fn version(&self) -> &str { "1.0.0" }
    fn required_permissions(&self) -> Vec<Permission> { vec![Permission::Shell] }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "metric": {
                    "type": "string",
                    "enum": ["cpu", "memory", "disk", "network", "processes", "all"],
                    "description": "Which metric to query (default: all)"
                },
                "process_count": {
                    "type": "integer",
                    "description": "Number of top processes to show (default: 10, max: 50)"
                }
            }
        })
    }

    async fn execute(&self, _ctx: &SkillContext, args: serde_json::Value) -> Result<SkillResult, SkillError> {
        let metric = args.get("metric").and_then(|v| v.as_str()).unwrap_or("all");
        let process_count = args.get("process_count").and_then(|v| v.as_u64()).unwrap_or(10).min(50);

        let mut results = serde_json::json!({});

        match metric {
            "cpu" | "all" => {
                results["cpu"] = self.get_cpu_info().await?;
            }
            _ => {}
        }

        match metric {
            "memory" | "all" => {
                results["memory"] = self.get_memory_info().await?;
            }
            _ => {}
        }

        match metric {
            "disk" | "all" => {
                results["disk"] = self.get_disk_info().await?;
            }
            _ => {}
        }

        match metric {
            "network" | "all" => {
                results["network"] = self.get_network_info().await?;
            }
            _ => {}
        }

        match metric {
            "processes" | "all" => {
                results["processes"] = self.get_top_processes(process_count as usize).await?;
            }
            _ => {}
        }

        Ok(SkillResult::success(results))
    }
}

impl SystemMonitorSkill {
    async fn get_cpu_info(&self) -> Result<serde_json::Value, SkillError> {
        let cmd = if cfg!(target_os = "linux") {
            "cat /proc/cpuinfo | grep 'model name' | head -1; echo '---'; nproc; echo '---'; top -bn1 | grep 'Cpu(s)'"
        } else if cfg!(target_os = "macos") {
            "sysctl -n machdep.cpu.brand_string; echo '---'; sysctl -n hw.ncpu; echo '---'; top -l 1 -n 0 | grep 'CPU usage'"
        } else {
            "wmic cpu get Name,NumberOfCores /format:list"
        };

        let output = run_command(cmd).await?;
        Ok(serde_json::json!({
            "info": output,
            "platform": std::env::consts::OS
        }))
    }

    async fn get_memory_info(&self) -> Result<serde_json::Value, SkillError> {
        let cmd = if cfg!(target_os = "linux") {
            "free -h"
        } else if cfg!(target_os = "macos") {
            "vm_stat | head -10; echo '---'; sysctl hw.memsize"
        } else {
            "wmic OS get FreePhysicalMemory,TotalVisibleMemorySize /format:list"
        };

        let output = run_command(cmd).await?;
        Ok(serde_json::json!({ "info": output }))
    }

    async fn get_disk_info(&self) -> Result<serde_json::Value, SkillError> {
        let cmd = if cfg!(windows) {
            "wmic logicaldisk get size,freespace,caption /format:list"
        } else {
            "df -h"
        };

        let output = run_command(cmd).await?;
        Ok(serde_json::json!({ "info": output }))
    }

    async fn get_network_info(&self) -> Result<serde_json::Value, SkillError> {
        let cmd = if cfg!(target_os = "linux") {
            "ip -br addr 2>/dev/null || ifconfig | grep -E 'inet |flags='"
        } else if cfg!(target_os = "macos") {
            "ifconfig | grep -E 'inet |status:'"
        } else {
            "wmic nic where NetEnabled=true get Name,Speed /format:list"
        };

        let output = run_command(cmd).await?;
        Ok(serde_json::json!({ "info": output }))
    }

    async fn get_top_processes(&self, count: usize) -> Result<serde_json::Value, SkillError> {
        let cmd = if cfg!(target_os = "linux") {
            format!("ps aux --sort=-%mem | head -n {}", count + 1)
        } else if cfg!(target_os = "macos") {
            format!("ps aux -m | head -n {}", count + 1)
        } else {
            format!("wmic process get Name,ProcessId,WorkingSetSize /format:csv | sort /R | head -n {}", count + 1)
        };

        let output = run_command(&cmd).await?;
        Ok(serde_json::json!({
            "processes": output,
            "count": count
        }))
    }
}

async fn run_command(cmd: &str) -> Result<String, SkillError> {
    let output = tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .output()
        .await
        .map_err(|e| SkillError::Io(e.to_string()))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

impl Default for SystemMonitorSkill {
    fn default() -> Self { Self::new() }
}

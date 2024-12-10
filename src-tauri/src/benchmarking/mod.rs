use std::time::{Instant, Duration};
use std::path::PathBuf;
use std::fs::{self, OpenOptions, File};
use std::io::Write;
use chrono::Local;
use serde::Serialize;
use log::info;
use sysinfo::{System, SystemExt, CpuExt, ProcessExt};

#[derive(Debug, Serialize)]
pub struct ScanMetrics {
    pub start_time: String,
    pub total_duration_ms: u128,
    pub total_files: usize,
    pub files_per_second: f64,
    pub memory_usage_mb: f64,
    pub thread_count: usize,
    pub directory_path: String,
}

#[derive(Debug, Serialize)]
pub struct IndexMetrics {
    pub start_time: String,
    pub total_duration_ms: u128,
    pub total_files: usize,
    pub files_per_second: f64,
    pub memory_usage_mb: f64,
    pub thread_count: usize,
    pub chunk_size: usize,
    pub average_chunk_duration_ms: f64,
    pub total_chunks: usize,
    pub index_size_mb: f64,
}

#[derive(Debug, Serialize)]
pub struct BenchmarkReport {
    pub timestamp: String,
    pub scan_metrics: ScanMetrics,
    pub index_metrics: IndexMetrics,
    pub system_info: SystemInfo,
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub cpu_cores: usize,
    pub cpu_threads: usize,
    pub total_memory_mb: u64,
    pub os: String,
    pub cpu_model: String,
    pub cpu_frequency_mhz: u64,
    pub cpu_usage: f32,
}

pub struct Benchmarker {
    start_time: Instant,
    log_path: PathBuf,
    sys: System,
}

impl Benchmarker {
    pub fn new() -> Self {
        let app_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
            .expect("Failed to get app data directory");
        let log_path = app_dir.join("benchmarks");
        fs::create_dir_all(&log_path).expect("Failed to create benchmark directory");
        
        let mut sys = System::new_all();
        sys.refresh_all();
        
        Self {
            start_time: Instant::now(),
            log_path,
            sys,
        }
    }

    pub fn start_operation(&mut self) {
        self.start_time = Instant::now();
        self.sys.refresh_all();
    }

    pub fn record_scan_metrics(&mut self, total_files: usize, thread_count: usize, directory_path: String) -> ScanMetrics {
        self.sys.refresh_all();
        let duration = self.start_time.elapsed();
        let files_per_second = total_files as f64 / duration.as_secs_f64();
        
        ScanMetrics {
            start_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            total_duration_ms: duration.as_millis(),
            total_files,
            files_per_second,
            memory_usage_mb: self.get_memory_usage(),
            thread_count,
            directory_path,
        }
    }

    pub fn record_index_metrics(
        &mut self,
        total_files: usize,
        chunk_size: usize,
        total_chunks: usize,
        chunk_durations: &[Duration],
        thread_count: usize,
        index_path: &PathBuf,
    ) -> IndexMetrics {
        self.sys.refresh_all();
        let duration = self.start_time.elapsed();
        let files_per_second = total_files as f64 / duration.as_secs_f64();
        let avg_chunk_duration = chunk_durations.iter()
            .map(|d| d.as_millis() as f64)
            .sum::<f64>() / chunk_durations.len() as f64;
        
        let index_size = self.get_directory_size(index_path) as f64 / (1024.0 * 1024.0);
        
        IndexMetrics {
            start_time: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            total_duration_ms: duration.as_millis(),
            total_files,
            files_per_second,
            memory_usage_mb: self.get_memory_usage(),
            thread_count,
            chunk_size,
            average_chunk_duration_ms: avg_chunk_duration,
            total_chunks,
            index_size_mb: index_size,
        }
    }

    pub fn save_benchmark_report(&mut self, scan_metrics: ScanMetrics, index_metrics: IndexMetrics) {
        self.sys.refresh_all();
        
        let report = BenchmarkReport {
            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            scan_metrics,
            index_metrics,
            system_info: self.get_system_info(),
        };

        let json = serde_json::to_string_pretty(&report).expect("Failed to serialize benchmark report");
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let file_path = self.log_path.join(format!("benchmark_{}.json", timestamp));
        
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&file_path)
            .expect("Failed to create benchmark file");
            
        file.write_all(json.as_bytes()).expect("Failed to write benchmark data");
        
        // Also write a human-readable summary
        let summary_path = self.log_path.join(format!("benchmark_{}_summary.txt", timestamp));
        self.write_summary(&report, &summary_path);
        
        info!("Benchmark report saved to: {:?}", file_path);
        info!("Summary saved to: {:?}", summary_path);
    }

    fn write_summary(&self, report: &BenchmarkReport, path: &PathBuf) {
        let mut file = File::create(path).expect("Failed to create summary file");
        
        writeln!(file, "=== Constella Benchmark Report ===").unwrap();
        writeln!(file, "Timestamp: {}", report.timestamp).unwrap();
        writeln!(file).unwrap();
        
        writeln!(file, "System Information:").unwrap();
        writeln!(file, "  CPU: {}", report.system_info.cpu_model).unwrap();
        writeln!(file, "  Physical Cores: {}", report.system_info.cpu_cores).unwrap();
        writeln!(file, "  Logical Cores: {}", report.system_info.cpu_threads).unwrap();
        writeln!(file, "  CPU Frequency: {:.2} GHz", report.system_info.cpu_frequency_mhz as f64 / 1000.0).unwrap();
        writeln!(file, "  CPU Usage: {:.1}%", report.system_info.cpu_usage).unwrap();
        writeln!(file, "  Memory: {:.2} GB", report.system_info.total_memory_mb as f64 / 1024.0).unwrap();
        writeln!(file, "  OS: {}", report.system_info.os).unwrap();
        writeln!(file).unwrap();
        
        writeln!(file, "Directory Scan Metrics:").unwrap();
        writeln!(file, "  Duration: {:.2} seconds", report.scan_metrics.total_duration_ms as f64 / 1000.0).unwrap();
        writeln!(file, "  Files Scanned: {}", report.scan_metrics.total_files).unwrap();
        writeln!(file, "  Scan Speed: {:.2} files/second", report.scan_metrics.files_per_second).unwrap();
        writeln!(file, "  Thread Count: {}", report.scan_metrics.thread_count).unwrap();
        writeln!(file, "  Memory Usage: {:.2} MB", report.scan_metrics.memory_usage_mb).unwrap();
        writeln!(file, "  Directory: {}", report.scan_metrics.directory_path).unwrap();
        writeln!(file).unwrap();
        
        writeln!(file, "Indexing Metrics:").unwrap();
        writeln!(file, "  Duration: {:.2} seconds", report.index_metrics.total_duration_ms as f64 / 1000.0).unwrap();
        writeln!(file, "  Files Indexed: {}", report.index_metrics.total_files).unwrap();
        writeln!(file, "  Index Speed: {:.2} files/second", report.index_metrics.files_per_second).unwrap();
        writeln!(file, "  Thread Count: {}", report.index_metrics.thread_count).unwrap();
        writeln!(file, "  Memory Usage: {:.2} MB", report.index_metrics.memory_usage_mb).unwrap();
        writeln!(file, "  Chunk Size: {}", report.index_metrics.chunk_size).unwrap();
        writeln!(file, "  Total Chunks: {}", report.index_metrics.total_chunks).unwrap();
        writeln!(file, "  Avg Chunk Duration: {:.2} ms", report.index_metrics.average_chunk_duration_ms).unwrap();
        writeln!(file, "  Index Size: {:.2} MB", report.index_metrics.index_size_mb).unwrap();
    }

    fn get_memory_usage(&self) -> f64 {
        // Get current process memory usage
        if let Some(process) = self.sys.processes_by_exact_name("constella").next() {
            process.memory() as f64 / (1024.0 * 1024.0)
        } else {
            // Fallback to total system memory usage if process not found
            self.sys.used_memory() as f64 / (1024.0 * 1024.0)
        }
    }

    fn get_directory_size(&self, path: &PathBuf) -> u64 {
        let mut total_size = 0;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    total_size += fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                } else if path.is_dir() {
                    total_size += self.get_directory_size(&path);
                }
            }
        }
        total_size
    }

    fn get_system_info(&self) -> SystemInfo {
        let cpu = self.sys.cpus().first().expect("No CPU found");
        
        SystemInfo {
            cpu_cores: num_cpus::get_physical(),
            cpu_threads: num_cpus::get(),
            total_memory_mb: self.sys.total_memory() / 1024,
            os: std::env::consts::OS.to_string(),
            cpu_model: cpu.brand().to_string(),
            cpu_frequency_mhz: cpu.frequency(),
            cpu_usage: cpu.cpu_usage(),
        }
    }
} 
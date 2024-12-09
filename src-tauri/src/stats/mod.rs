use serde::{Serialize, Deserialize};
use std::time::{SystemTime, Duration};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub total_documents: u64,
    pub total_size: u64,
    pub file_types: HashMap<String, FileTypeStats>,
    pub indexing_history: Vec<IndexingOperation>,
    pub performance_metrics: PerformanceMetrics,
    pub system_metrics: SystemMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeStats {
    pub count: u64,
    pub total_size: u64,
    pub avg_processing_time: Duration,
    pub last_indexed: SystemTime,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingOperation {
    pub timestamp: SystemTime,
    pub operation_type: OperationType,
    pub files_processed: u32,
    pub duration: Duration,
    pub success_rate: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OperationType {
    InitialIndex,
    IncrementalUpdate,
    Reindex,
    Optimize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub avg_indexing_speed: f32,  // files per second
    pub avg_query_time: Duration,
    pub index_size_history: Vec<(SystemTime, u64)>,
    pub query_performance_history: Vec<QueryMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryMetric {
    pub timestamp: SystemTime,
    pub query_type: String,
    pub duration: Duration,
    pub results_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: Vec<(SystemTime, f32)>,
    pub memory_usage: Vec<(SystemTime, f32)>,
    pub io_operations: Vec<(SystemTime, u64)>,
}

impl IndexStats {
    pub fn new() -> Self {
        Self {
            total_documents: 0,
            total_size: 0,
            file_types: HashMap::new(),
            indexing_history: Vec::new(),
            performance_metrics: PerformanceMetrics::default(),
            system_metrics: SystemMetrics::default(),
        }
    }

    pub fn update_file_type_stats(&mut self, extension: String, size: u64, processing_time: Duration) {
        let stats = self.file_types.entry(extension).or_insert_with(|| FileTypeStats {
            count: 0,
            total_size: 0,
            avg_processing_time: Duration::default(),
            last_indexed: SystemTime::now(),
            error_count: 0,
        });

        stats.count += 1;
        stats.total_size += size;
        stats.avg_processing_time = (stats.avg_processing_time + processing_time) / 2;
        stats.last_indexed = SystemTime::now();
    }
} 
impl IndexManager {
    pub async fn get_status(&self) -> IndexProgress {
        let state = self.indexing_state.lock().await;
        IndexProgress {
            total_files: state.total_files,
            processed_files: state.processed_files,
            current_file: state.current_file.clone(),
            state: if state.is_complete {
                "Completed".to_string()
            } else if state.is_cancelled {
                "Cancelled".to_string()
            } else if state.is_paused {
                "Paused".to_string()
            } else if state.total_files == 0 {
                "Running".to_string()
            } else {
                "Running".to_string()
            },
            is_complete: state.is_complete
        }
    }
} 
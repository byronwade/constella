impl IndexManager {
    pub async fn pause(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_paused = true;
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_paused = false;
        Ok(())
    }

    pub async fn cancel(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_cancelled = true;
        Ok(())
    }

    pub async fn get_status(&self) -> IndexProgress {
        let state = self.indexing_state.lock().await;
        IndexProgress {
            total_files: state.total_files,
            processed_files: state.processed_files,
            current_file: state.current_file.clone(),
            state: if state.is_complete {
                "Complete".to_string()
            } else if state.is_cancelled {
                "Cancelled".to_string()
            } else if state.is_paused {
                "Paused".to_string()
            } else if state.total_files == 0 {
                "Counting".to_string()
            } else {
                "Indexing".to_string()
            },
            is_complete: state.is_complete
        }
    }

    async fn send_progress(&self, progress_tx: &mpsc::Sender<IndexProgress>) {
        let progress = self.get_status().await;
        let _ = progress_tx.try_send(progress);
    }
} 
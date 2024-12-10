export interface IndexingProgress {
	state: "idle" | "scanning" | "indexing" | "completed" | "error";
	stats: {
		total_files: number;
		processed_files: number;
		percent_complete: number;
		files_per_second: number;
		elapsed_seconds: number;
		estimated_remaining_seconds: number | null;
	};
	current_file: string;
}

export interface SearchResult {
	path: string;
	name: string;
	size: number;
	modified: number;
	score: number;
}

export interface IndexStats {
	total_documents: number;
	last_updated: string;
}

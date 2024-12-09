export interface IndexingStatus {
	total_files: number;
	processed_files: number;
	current_file: string;
	state: "Idle" | "Scanning" | "Running" | "Complete" | "Cancelled" | "Paused" | "Error";
	is_complete: boolean;
	files_found?: number;
	start_time: number;
}

export interface SearchMatch {
	line: number;
	content: string;
}

export interface SearchResult {
	path: string;
	name: string;
	is_dir: boolean;
	size: number;
	size_formatted: string;
	modified_formatted: string;
	matches?: SearchMatch[];
}

export interface IndexStats {
	total_documents: number;
	total_size: number;
	last_updated: string | null;
}

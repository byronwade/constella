export interface IndexingStatus {
	total_files: number;
	processed_files: number;
	current_file: string;
	state: "Idle" | "Running" | "Complete" | "Error";
	is_complete: boolean;
}

export interface SearchResult {
	path: string;
	name: string;
	type: "file" | "directory";
	size: number;
	modified: string;
	matches?: {
		line: number;
		content: string;
	}[];
}

export interface IndexStats {
	total_documents: number;
	total_size: number;
	last_updated: string | null;
}

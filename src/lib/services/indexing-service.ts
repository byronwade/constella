import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type { IndexingProgress, IndexStats } from "../types";

export class IndexingService {
	private static instance: IndexingService;
	private progressListeners: ((progress: IndexingProgress) => void)[] = [];
	private unlistenProgress?: () => void;

	private constructor() {
		// Set up event listener once
		listen<IndexingProgress>("indexing_progress", (event) => {
			this.progressListeners.forEach((listener) => listener(event.payload));
		}).then((unlisten) => {
			this.unlistenProgress = unlisten;
		});
	}

	static getInstance(): IndexingService {
		if (!IndexingService.instance) {
			IndexingService.instance = new IndexingService();
		}
		return IndexingService.instance;
	}

	async startIndexing(directory: string): Promise<void> {
		try {
			await invoke("start_indexing", { directory });
		} catch (error) {
			console.error("Failed to start indexing:", error);
			throw new Error(`Failed to start indexing: ${error}`);
		}
	}

	async getIndexingProgress(): Promise<IndexingProgress> {
		try {
			return await invoke("get_indexing_progress");
		} catch (error) {
			console.error("Failed to get indexing progress:", error);
			throw new Error(`Failed to get indexing progress: ${error}`);
		}
	}

	async cancelIndexing(): Promise<void> {
		try {
			await invoke("cancel_indexing");
		} catch (error) {
			console.error("Failed to cancel indexing:", error);
			throw new Error(`Failed to cancel indexing: ${error}`);
		}
	}

	async getIndexStats(): Promise<IndexStats> {
		try {
			return await invoke("get_index_stats");
		} catch (error) {
			console.error("Failed to get index stats:", error);
			throw new Error(`Failed to get index stats: ${error}`);
		}
	}

	onProgress(callback: (progress: IndexingProgress) => void): () => void {
		this.progressListeners.push(callback);

		// Return cleanup function
		return () => {
			this.progressListeners = this.progressListeners.filter((listener) => listener !== callback);
		};
	}

	cleanup(): void {
		if (this.unlistenProgress) {
			this.unlistenProgress();
		}
		this.progressListeners = [];
	}
}

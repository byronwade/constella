import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type { IndexingStatus, IndexStats } from "../types";

export class IndexingService {
	private static instance: IndexingService;
	private progressCallback?: (status: IndexingStatus) => void;
	private unlistenFn?: () => void;

	private constructor() {}

	static getInstance(): IndexingService {
		if (!IndexingService.instance) {
			IndexingService.instance = new IndexingService();
		}
		return IndexingService.instance;
	}

	async startIndexing(path: string): Promise<void> {
		console.log("Starting indexing for path:", path);
		await invoke("index_directory", { path });
	}

	async pauseIndexing(): Promise<void> {
		console.log("Pausing indexing");
		await invoke("pause_indexing");
	}

	async resumeIndexing(): Promise<void> {
		console.log("Resuming indexing");
		await invoke("resume_indexing");
	}

	async cancelIndexing(): Promise<void> {
		console.log("Cancelling indexing");
		await invoke("cancel_indexing");
	}

	async getIndexStats(): Promise<IndexStats> {
		return invoke("get_index_stats");
	}

	async listenToProgress(callback: (status: IndexingStatus) => void): Promise<void> {
		// Cleanup previous listener if exists
		if (this.unlistenFn) {
			this.unlistenFn();
		}

		this.progressCallback = callback;
		this.unlistenFn = await listen<IndexingStatus>("indexing-progress", (event) => {
			console.log("Indexing progress:", event.payload);
			if (this.progressCallback) {
				this.progressCallback(event.payload);
			}
		});
	}

	cleanup(): void {
		if (this.unlistenFn) {
			this.unlistenFn();
		}
	}
}

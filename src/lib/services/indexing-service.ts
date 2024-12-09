import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type { IndexingStatus, IndexStats } from "../types";

export class IndexingService {
	private static instance: IndexingService;
	private progressCallback?: (status: IndexingStatus) => void;
	private unlistenFn?: () => void;
	private startTime?: number;
	private lastUpdate: number = 0;
	private readonly UPDATE_INTERVAL = 100; // ms
	private readonly MIN_SPEED_CALCULATION_TIME = 1000; // ms, minimum time before calculating speed

	private constructor() {}

	static getInstance(): IndexingService {
		if (!IndexingService.instance) {
			IndexingService.instance = new IndexingService();
		}
		return IndexingService.instance;
	}

	async startIndexing(path: string): Promise<void> {
		try {
			console.log("Starting indexing for path:", path);
			this.startTime = Date.now();
			this.lastUpdate = 0;
			await invoke("start_indexing", { path });
		} catch (error) {
			console.error("Failed to start indexing:", error);
			throw new Error(`Failed to start indexing: ${error}`);
		}
	}

	async pauseIndexing(): Promise<void> {
		try {
			console.log("Pausing indexing");
			await invoke("pause_indexing");
		} catch (error) {
			console.error("Failed to pause indexing:", error);
			throw new Error(`Failed to pause indexing: ${error}`);
		}
	}

	async resumeIndexing(): Promise<void> {
		try {
			console.log("Resuming indexing");
			await invoke("resume_indexing");
		} catch (error) {
			console.error("Failed to resume indexing:", error);
			throw new Error(`Failed to resume indexing: ${error}`);
		}
	}

	async cancelIndexing(): Promise<void> {
		try {
			console.log("Cancelling indexing");
			await invoke("cancel_indexing");
		} catch (error) {
			console.error("Failed to cancel indexing:", error);
			throw new Error(`Failed to cancel indexing: ${error}`);
		}
	}

	async getIndexStats(): Promise<IndexStats> {
		try {
			return await invoke("verify_index");
		} catch (error) {
			console.error("Failed to get index stats:", error);
			throw new Error(`Failed to get index stats: ${error}`);
		}
	}

	async listenToProgress(callback: (status: IndexingStatus) => void): Promise<void> {
		// Cleanup previous listener if exists
		if (this.unlistenFn) {
			this.unlistenFn();
		}

		this.progressCallback = (status: IndexingStatus) => {
			const now = Date.now();

			// Throttle updates to avoid UI jank
			if (now - this.lastUpdate < this.UPDATE_INTERVAL && !status.is_complete) {
				return;
			}

			// Calculate speed and ETA
			const elapsedTime = now - (this.startTime || now);
			const elapsedSeconds = elapsedTime / 1000;

			// Only calculate speed after minimum time has passed
			let speed = 0;
			let eta: string | undefined;
			let percent = "0.0";

			if (elapsedTime >= this.MIN_SPEED_CALCULATION_TIME && status.processed_files > 0) {
				speed = status.processed_files / elapsedSeconds;

				if (status.total_files > 0) {
					const remainingFiles = status.total_files - status.processed_files;
					if (speed > 0) {
						eta = this.formatTime(remainingFiles / speed);
					}
					percent = ((status.processed_files / status.total_files) * 100).toFixed(1);
				}
			}

			// Enhance status with additional metrics
			const enhancedStatus = {
				...status,
				speed: speed.toFixed(1),
				eta,
				elapsed: this.formatTime(elapsedSeconds),
				percent,
			};

			// Don't log every update to avoid console spam
			if (status.is_complete || now - this.lastUpdate >= 1000) {
				console.log("Indexing progress:", enhancedStatus);
			}

			callback(enhancedStatus);
			this.lastUpdate = now;
		};

		try {
			this.unlistenFn = await listen<IndexingStatus>("indexing-progress", (event) => {
				if (this.progressCallback) {
					this.progressCallback(event.payload);
				}
			});
		} catch (error) {
			console.error("Failed to setup progress listener:", error);
			throw new Error(`Failed to setup progress listener: ${error}`);
		}
	}

	private formatTime(seconds: number): string {
		if (!isFinite(seconds) || seconds < 0) return "calculating...";
		if (seconds < 60) return `${Math.round(seconds)}s`;
		const minutes = Math.floor(seconds / 60);
		const remainingSeconds = Math.round(seconds % 60);
		if (minutes < 60) return `${minutes}m ${remainingSeconds}s`;
		const hours = Math.floor(minutes / 60);
		const remainingMinutes = minutes % 60;
		return `${hours}h ${remainingMinutes}m`;
	}

	cleanup(): void {
		try {
			if (this.unlistenFn) {
				this.unlistenFn();
			}
		} catch (error) {
			console.error("Error during cleanup:", error);
		} finally {
			this.startTime = undefined;
			this.lastUpdate = 0;
		}
	}
}

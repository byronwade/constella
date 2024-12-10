import { useState, useEffect } from "react";
import { open } from "@tauri-apps/api/dialog";
import { Button } from "@/components/ui/button";
import { IndexingProgress } from "./indexing-progress";
import { IndexingStatus } from "@/lib/types";
import { Card, CardContent } from "@/components/ui/card";
import { IndexingService } from "@/lib/services/indexing-service";

export function DirectoryIndexer() {
	const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [indexingStatus, setIndexingStatus] = useState<IndexingStatus>({
		total_files: 0,
		processed_files: 0,
		files_found: 0,
		current_file: "",
		state: "Idle",
		is_complete: false,
		start_time: Date.now(),
	});

	// Initialize indexing service and progress listener
	useEffect(() => {
		const indexingService = IndexingService.getInstance();
		indexingService
			.listenToProgress((status) => {
				console.log("Received indexing status:", status);
				setIndexingStatus((prev) => ({
					...prev,
					...status,
				}));

				// Update loading state based on status
				if (status.state === "Complete" || status.state === "Error" || status.state === "Cancelled") {
					setIsLoading(false);
				}
			})
			.catch((error) => {
				console.error("Failed to setup progress listener:", error);
				setError("Failed to setup progress listener");
			});

		// Cleanup listener on unmount
		return () => {
			indexingService.cleanup();
		};
	}, []);

	const handleSelectDirectory = async () => {
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				defaultPath: "/",
			});

			if (selected) {
				setSelectedDirectory(selected as string);
				setError(null);
			}
		} catch (e) {
			console.error("Failed to select directory:", e);
			setError("Failed to select directory");
		}
	};

	const handleStartIndexing = async () => {
		if (!selectedDirectory) return;

		try {
			setIsLoading(true);
			setError(null);
			setIndexingStatus((prev) => ({
				...prev,
				state: "Scanning",
				is_complete: false,
				start_time: Date.now(),
				total_files: 0,
				processed_files: 0,
				files_found: 0,
			}));

			const indexingService = IndexingService.getInstance();
			console.log("Starting indexing with service for path:", selectedDirectory);
			await indexingService.startIndexing(selectedDirectory);
		} catch (e) {
			console.error("Failed to start indexing:", e);
			setError("Failed to start indexing");
			setIndexingStatus((prev) => ({
				...prev,
				state: "Error",
				is_complete: false,
			}));
			setIsLoading(false);
		}
	};

	return (
		<Card>
			<CardContent className="pt-6 space-y-4">
				<div className="flex flex-col gap-4">
					<div className="flex items-center gap-4">
						<Button onClick={handleSelectDirectory} disabled={isLoading || indexingStatus.state === "Running" || indexingStatus.state === "Scanning"}>
							{selectedDirectory ? "Change Directory" : "Select Directory"}
						</Button>
						{selectedDirectory && (
							<Button onClick={handleStartIndexing} disabled={isLoading || indexingStatus.state === "Running" || indexingStatus.state === "Scanning"}>
								{isLoading ? "Starting..." : "Start Indexing"}
							</Button>
						)}
					</div>

					{selectedDirectory && <div className="text-sm text-muted-foreground">Selected: {selectedDirectory}</div>}

					{error && <div className="text-sm text-red-500">{error}</div>}

					{(indexingStatus.state !== "Idle" || indexingStatus.is_complete) && <IndexingProgress status={indexingStatus} />}
				</div>
			</CardContent>
		</Card>
	);
}

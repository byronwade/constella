import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open } from "@tauri-apps/api/dialog";
import { Button } from "@/components/ui/button";
import { IndexingProgress } from "./indexing-progress";
import { IndexingStatus } from "@/lib/types";
import { listen } from "@tauri-apps/api/event";
import { Card, CardContent } from "@/components/ui/card";

export function DirectoryIndexer() {
	const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
	const [isLoading, setIsLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [indexingStatus, setIndexingStatus] = useState<IndexingStatus>({
		total_files: 0,
		processed_files: 0,
		files_found: 0,
		current_file: "",
		state: "Idle" as const,
		is_complete: false,
		start_time: Date.now(),
	});

	// Listen for indexing progress events
	useEffect(() => {
		const unlisten = listen<IndexingStatus>("indexing-progress", (event) => {
			setIndexingStatus((prev) => ({
				...prev,
				...event.payload,
				start_time: prev.start_time, // Preserve the original start time
			}));
		});

		return () => {
			unlisten.then((fn) => fn());
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
				state: "Scanning" as const,
				is_complete: false,
				start_time: Date.now(),
				total_files: 0,
				processed_files: 0,
				files_found: 0,
			}));

			await invoke("start_indexing", { path: selectedDirectory });
		} catch (e) {
			console.error("Failed to start indexing:", e);
			setError("Failed to start indexing");
			setIndexingStatus((prev) => ({
				...prev,
				state: "Error" as const,
				is_complete: false,
			}));
		} finally {
			setIsLoading(false);
		}
	};

	return (
		<Card>
			<CardContent className="pt-6 space-y-4">
				<div className="flex flex-col gap-4">
					<div className="flex items-center gap-4">
						<Button onClick={handleSelectDirectory} disabled={isLoading}>
							{selectedDirectory ? "Change Directory" : "Select Directory"}
						</Button>
						{selectedDirectory && (
							<Button onClick={handleStartIndexing} disabled={isLoading}>
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

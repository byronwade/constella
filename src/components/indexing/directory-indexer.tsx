import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open } from "@tauri-apps/api/dialog";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { IndexingProgress } from "@/lib/types";
import { formatDuration, formatNumber } from "@/lib/utils";

export function DirectoryIndexer() {
	const [selectedDirectory, setSelectedDirectory] = useState<string | null>(null);
	const [progress, setProgress] = useState<IndexingProgress | null>(null);
	const [isIndexing, setIsIndexing] = useState(false);

	useEffect(() => {
		let interval: NodeJS.Timeout;

		if (isIndexing) {
			interval = setInterval(async () => {
				try {
					const progress = await invoke<IndexingProgress>("get_indexing_progress");
					setProgress(progress);

					if (progress.state === "completed" || progress.state === "error") {
						setIsIndexing(false);
					}
				} catch (error) {
					console.error("Failed to get progress:", error);
				}
			}, 100);
		}

		return () => {
			if (interval) clearInterval(interval);
		};
	}, [isIndexing]);

	const handleSelectDirectory = async () => {
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				defaultPath: "~",
			});
			if (selected && typeof selected === "string") {
				setSelectedDirectory(selected);
			}
		} catch (error) {
			console.error("Failed to select directory:", error);
		}
	};

	const handleStartIndexing = async () => {
		if (!selectedDirectory) return;

		try {
			setIsIndexing(true);
			await invoke("start_indexing", { directory: selectedDirectory });
		} catch (error) {
			console.error("Failed to start indexing:", error);
			setIsIndexing(false);
		}
	};

	const getProgressPercentage = () => {
		if (!progress || progress.stats.total_files === 0) return 0;
		return Math.min(100, (progress.stats.processed_files / progress.stats.total_files) * 100);
	};

	return (
		<div className="space-y-4">
			<div className="flex flex-col gap-2">
				<Button onClick={handleSelectDirectory} disabled={isIndexing}>
					Select Directory
				</Button>
				{selectedDirectory && <div className="text-sm text-muted-foreground truncate">Selected: {selectedDirectory}</div>}
			</div>

			{progress && (
				<div className="space-y-4">
					<div className="flex justify-between items-center">
						<div className="text-sm font-medium">{progress.state === "completed" ? "Completed" : "Indexing..."}</div>
						<div className="text-sm text-muted-foreground">{getProgressPercentage().toFixed(1)}%</div>
					</div>

					<Progress value={getProgressPercentage()} className="h-2" />

					<div className="grid grid-cols-2 gap-2 text-sm">
						<div>
							Processed: {formatNumber(progress.stats.processed_files)} / {formatNumber(progress.stats.total_files)} files
						</div>
						<div>Speed: {formatNumber(Math.round(progress.stats.files_per_second))} files/sec</div>
						<div>Time: {formatDuration(progress.stats.elapsed_seconds)}</div>
						{progress.current_file && <div className="col-span-2 truncate text-muted-foreground">{progress.current_file}</div>}
					</div>
				</div>
			)}

			<Button onClick={handleStartIndexing} disabled={!selectedDirectory || isIndexing} className="w-full">
				{isIndexing ? "Indexing..." : "Start Indexing"}
			</Button>
		</div>
	);
}

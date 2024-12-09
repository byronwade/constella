import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open } from "@tauri-apps/api/dialog";
import { listen } from "@tauri-apps/api/event";
import { Button } from "../ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "../ui/card";
import { IndexingProgress } from "./indexing-progress";
import { Folder, Loader2 } from "lucide-react";
import { useToast } from "@/hooks/use-toast";
import { IndexingStatus } from "@/lib/types";

export function DirectoryIndexer() {
	const { toast } = useToast();
	const [selectedPath, setSelectedPath] = useState<string>("");
	const [isLoading, setIsLoading] = useState(false);
	const [indexingStatus, setIndexingStatus] = useState<IndexingStatus>({
		total_files: 0,
		processed_files: 0,
		current_file: "",
		state: "Idle",
		is_complete: false,
	});

	// Listen for indexing progress events
	useEffect(() => {
		let unlisten: (() => void) | undefined;

		const setupListeners = async () => {
			// Progress updates
			unlisten = await listen<IndexingStatus>("indexing-progress", (event) => {
				const status = event.payload;
				setIndexingStatus(status);
			});

			// Completion listener
			const unlistenComplete = await listen("indexing-complete", () => {
				setIndexingStatus((prev) => ({
					...prev,
					state: "Complete",
					is_complete: true,
				}));
				setIsLoading(false);
				toast({
					title: "Indexing Complete",
					description: `Successfully indexed ${indexingStatus.total_files.toLocaleString()} files`,
				});
			});

			// Error listener
			const unlistenError = await listen("indexing-error", (event: any) => {
				console.error("Indexing error:", event);
				setIndexingStatus((prev) => ({
					...prev,
					state: "Error",
					is_complete: false,
				}));
				setIsLoading(false);
				toast({
					title: "Indexing Error",
					description: event.payload || "An error occurred during indexing",
					variant: "destructive",
				});
			});

			return () => {
				if (unlisten) unlisten();
				unlistenComplete();
				unlistenError();
			};
		};

		setupListeners().catch(console.error);

		return () => {
			if (unlisten) unlisten();
		};
	}, [toast, indexingStatus.total_files]);

	const handleSelectDirectory = async () => {
		try {
			const selected = await open({
				directory: true,
				multiple: false,
				title: "Select Directory to Index",
			});

			if (selected && typeof selected === "string") {
				setSelectedPath(selected);
			}
		} catch (error) {
			console.error("Failed to select directory:", error);
			toast({
				title: "Error",
				description: "Failed to select directory",
				variant: "destructive",
			});
		}
	};

	const handleStartIndexing = async () => {
		if (!selectedPath) {
			return toast({
				title: "No Directory Selected",
				description: "Please select a directory to index",
				variant: "destructive",
			});
		}

		try {
			setIsLoading(true);
			setIndexingStatus((prev) => ({
				...prev,
				state: "Running",
				processed_files: 0,
				total_files: 0,
				current_file: "Starting indexing...",
			}));

			await invoke("start_indexing", { path: selectedPath });
		} catch (error) {
			console.error("Failed to start indexing:", error);
			setIndexingStatus((prev) => ({
				...prev,
				state: "Error",
			}));
			setIsLoading(false);
			toast({
				title: "Error",
				description: error instanceof Error ? error.message : "Failed to start indexing",
				variant: "destructive",
			});
		}
	};

	return (
		<Card>
			<CardHeader>
				<CardTitle>Directory Indexer</CardTitle>
				<CardDescription>Select a directory to index its contents</CardDescription>
			</CardHeader>
			<CardContent className="space-y-4">
				<div className="flex items-center space-x-4">
					<Button onClick={handleSelectDirectory} disabled={isLoading} variant="outline">
						<Folder className="mr-2 h-4 w-4" />
						Select Directory
					</Button>
					<Button onClick={handleStartIndexing} disabled={!selectedPath || isLoading}>
						{isLoading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : "Start Indexing"}
					</Button>
				</div>

				{selectedPath && <div className="text-sm text-muted-foreground">Selected: {selectedPath}</div>}

				{indexingStatus.state !== "Idle" && <IndexingProgress status={indexingStatus} />}
			</CardContent>
		</Card>
	);
}

import { Progress } from "../ui/progress";
import { IndexingStatus } from "@/lib/types";
import { Loader2 } from "lucide-react";

interface IndexingProgressProps {
	status: IndexingStatus;
}

export function IndexingProgress({ status }: IndexingProgressProps) {
	const progress = status.total_files > 0 ? Math.round((status.processed_files / status.total_files) * 100) : 0;
	const formattedTotal = status.total_files.toLocaleString();
	const formattedProcessed = status.processed_files.toLocaleString();
	const formattedFound = status.files_found?.toLocaleString() || "0";

	return (
		<div className="space-y-2">
			<div className="flex items-center justify-between text-sm">
				<div className="flex items-center gap-2">
					{(status.state === "Running" || status.state === "Scanning") && <Loader2 className="h-4 w-4 animate-spin" />}
					<span>
						{status.state === "Scanning" && "Scanning files..."}
						{status.state === "Running" && "Indexing..."}
						{status.state === "Complete" && "Indexing complete"}
						{status.state === "Error" && "Error during indexing"}
						{status.state === "Paused" && "Indexing paused"}
						{status.state === "Cancelled" && "Indexing cancelled"}
						{status.state === "Idle" && "Ready to index"}
					</span>
				</div>
				<span className="font-medium">{progress}%</span>
			</div>

			<Progress value={progress} className="h-2" />

			{status.state === "Running" && status.current_file && (
				<div className="text-sm text-muted-foreground truncate">
					<div className="font-medium">Current file:</div>
					<div className="truncate">{status.current_file}</div>
				</div>
			)}

			<div className="text-sm text-muted-foreground">
				{status.state === "Scanning" ? (
					<>Found {formattedFound} files</>
				) : (
					<div className="flex flex-col gap-1">
						<div>
							Processed: {formattedProcessed} / {formattedTotal} files
						</div>
						<div className="text-xs">{status.processed_files > 0 && <>Speed: {Math.round(status.processed_files / ((Date.now() - status.start_time) / 1000))} files/sec</>}</div>
					</div>
				)}
			</div>
		</div>
	);
}

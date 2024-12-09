import { Progress } from "../ui/progress";
import { IndexingStatus } from "@/lib/types";
import { Loader2 } from "lucide-react";

interface IndexingProgressProps {
	status: IndexingStatus;
}

export function IndexingProgress({ status }: IndexingProgressProps) {
	const progress = status.total_files > 0 ? Math.round((status.processed_files / status.total_files) * 100) : 0;

	return (
		<div className="space-y-2">
			<div className="flex items-center justify-between text-sm">
				<div className="flex items-center gap-2">
					{status.state === "Running" && <Loader2 className="h-4 w-4 animate-spin" />}
					<span>
						{status.state === "Running" && "Indexing..."}
						{status.state === "Complete" && "Indexing complete"}
						{status.state === "Error" && "Error during indexing"}
					</span>
				</div>
				<span className="font-medium">{progress}%</span>
			</div>

			<Progress value={progress} className="h-2" />

			{status.state === "Running" && status.current_file && <div className="text-sm text-muted-foreground truncate">{status.current_file}</div>}

			<div className="text-sm text-muted-foreground">
				{status.processed_files.toLocaleString()} / {status.total_files.toLocaleString()} files
			</div>
		</div>
	);
}

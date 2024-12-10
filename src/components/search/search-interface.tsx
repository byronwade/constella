import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Search as SearchIcon, FileIcon, Loader2 } from "lucide-react";
import { SearchResult } from "@/lib/types";
import { debounce } from "lodash";
import { formatFileSize, formatDate } from "@/lib/utils";

export function SearchInterface() {
	const [query, setQuery] = useState("");
	const [results, setResults] = useState<SearchResult[]>([]);
	const [isSearching, setIsSearching] = useState(false);

	const debouncedSearch = useCallback(
		debounce(async (searchQuery: string) => {
			if (!searchQuery.trim()) {
				setResults([]);
				return;
			}

			setIsSearching(true);
			try {
				const searchResults = await invoke<SearchResult[]>("search_files", {
					query: searchQuery,
				});
				setResults(searchResults);
			} catch (error) {
				console.error("Search failed:", error);
			} finally {
				setIsSearching(false);
			}
		}, 300),
		[]
	);

	useEffect(() => {
		debouncedSearch(query);
		return () => debouncedSearch.cancel();
	}, [query, debouncedSearch]);

	return (
		<div className="w-full max-w-2xl mx-auto p-4">
			<div className="flex flex-col gap-4">
				<div className="relative">
					<Input type="text" placeholder="Search files..." value={query} onChange={(e) => setQuery(e.target.value)} className="w-full pl-10 pr-4 py-2 text-lg" autoFocus />
					<div className="absolute left-3 top-1/2 -translate-y-1/2">{isSearching ? <Loader2 className="h-5 w-5 animate-spin text-muted-foreground" /> : <SearchIcon className="h-5 w-5 text-muted-foreground" />}</div>
				</div>

				{results.length > 0 && (
					<div className="space-y-2 mt-4">
						{results.map((result, index) => (
							<div key={`${result.path}-${index}`} className="flex items-start gap-3 p-3 rounded-lg bg-card hover:bg-accent/50 transition-colors">
								<FileIcon className="h-5 w-5 mt-0.5 text-muted-foreground shrink-0" />
								<div className="flex-1 min-w-0">
									<div className="font-medium truncate">{result.name}</div>
									<div className="text-sm text-muted-foreground truncate">{result.path}</div>
									<div className="text-xs text-muted-foreground mt-1 flex gap-2">
										<span>{formatFileSize(result.size)}</span>
										<span>•</span>
										<span>{formatDate(result.modified * 1000)}</span>
										<span>•</span>
										<span>Score: {result.score.toFixed(2)}</span>
									</div>
								</div>
							</div>
						))}
					</div>
				)}

				{query && !isSearching && results.length === 0 && <div className="text-center py-8 text-muted-foreground">No results found for "{query}"</div>}
			</div>
		</div>
	);
}

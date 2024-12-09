import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Search as SearchIcon, FileIcon, FolderIcon, Loader2 } from "lucide-react";
import { SearchResult } from "@/lib/types";
import { debounce } from "lodash";

export function SearchInterface() {
	const [query, setQuery] = useState("");
	const [results, setResults] = useState<SearchResult[]>([]);
	const [isSearching, setIsSearching] = useState(false);

	// Debounced search function
	const debouncedSearch = useCallback(
		debounce(async (searchQuery: string) => {
			if (!searchQuery.trim()) {
				setResults([]);
				return;
			}

			setIsSearching(true);
			try {
				console.log("Executing search for query:", searchQuery);
				const searchResults = await invoke<SearchResult[]>("search_files", {
					query: searchQuery,
				});
				console.log("Search results:", searchResults);
				setResults(searchResults);
			} catch (error) {
				console.error("Search failed:", error);
			} finally {
				setIsSearching(false);
			}
		}, 300),
		[]
	);

	// Effect to trigger search on query change
	useEffect(() => {
		debouncedSearch(query);
		return () => debouncedSearch.cancel();
	}, [query, debouncedSearch]);

	return (
		<div className="flex flex-col w-full max-w-2xl gap-4">
			<div className="relative">
				<Input type="search" placeholder="Search files..." value={query} onChange={(e) => setQuery(e.target.value)} className="pl-10" />
				<div className="absolute left-3 top-1/2 -translate-y-1/2">{isSearching ? <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" /> : <SearchIcon className="h-4 w-4 text-muted-foreground" />}</div>
			</div>

			{/* Results section */}
			{results.length > 0 && (
				<div className="border rounded-lg divide-y">
					{results.map((result, index) => (
						<div key={index} className="p-4 hover:bg-muted/50 transition-colors">
							<div className="flex items-start gap-3">
								{result.is_dir ? <FolderIcon className="h-5 w-5 mt-1 text-muted-foreground" /> : <FileIcon className="h-5 w-5 mt-1 text-muted-foreground" />}
								<div className="flex-1 min-w-0">
									<h3 className="font-medium truncate">{result.name}</h3>
									<p className="text-sm text-muted-foreground truncate">{result.path}</p>
									{result.matches?.map((match, matchIndex) => (
										<div key={matchIndex} className="mt-2 text-sm bg-muted/50 p-2 rounded">
											<span className="text-muted-foreground">Line {match.line}:</span>
											<pre className="mt-1 whitespace-pre-wrap font-mono text-xs">{match.content}</pre>
										</div>
									))}
									<div className="mt-2 flex gap-4 text-xs text-muted-foreground">
										<span>{result.size_formatted}</span>
										<span>Modified: {result.modified_formatted}</span>
									</div>
								</div>
							</div>
						</div>
					))}
				</div>
			)}

			{query && !isSearching && results.length === 0 && <div className="text-center py-8 text-muted-foreground">No results found for "{query}"</div>}
		</div>
	);
}

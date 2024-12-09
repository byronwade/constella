import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useToast } from "@/hooks/use-toast";
import { Search as SearchIcon } from "lucide-react";

export function SearchInterface() {
	const [query, setQuery] = useState("");
	const { toast } = useToast();

	const handleSearch = async (e: React.FormEvent) => {
		e.preventDefault();

		if (!query.trim()) {
			return toast({
				title: "Search query required",
				description: "Please enter a search term",
				variant: "destructive",
			});
		}

		// TODO: Implement search functionality
		toast({
			title: "Searching...",
			description: `Looking for: ${query}`,
		});
	};

	return (
		<form onSubmit={handleSearch} className="flex w-full max-w-2xl gap-2">
			<Input type="search" placeholder="Search files..." value={query} onChange={(e) => setQuery(e.target.value)} className="flex-1" />
			<Button type="submit" variant="default">
				<SearchIcon className="h-4 w-4 mr-2" />
				Search
			</Button>
		</form>
	);
}

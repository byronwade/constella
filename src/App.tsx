import { DirectoryIndexer } from "@/components/indexing/directory-indexer";
import { SearchInterface } from "@/components/search/search-interface";

export default function App() {
	return (
		<main className="min-h-screen bg-background">
			<div className="container py-8 space-y-8">
				<div>
					<h1 className="text-3xl font-bold mb-2">Constella File Search</h1>
					<p className="text-muted-foreground">Fast, efficient file search for your system.</p>
				</div>

				<div className="grid gap-8 md:grid-cols-2">
					<div className="space-y-4">
						<h2 className="text-xl font-semibold">Index Management</h2>
						<DirectoryIndexer />
					</div>

					<div className="space-y-4">
						<h2 className="text-xl font-semibold">Search Files</h2>
						<SearchInterface />
					</div>
				</div>
			</div>
		</main>
	);
}

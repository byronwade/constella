import { SearchInterface } from "./components/search/search-interface";
import { DirectoryIndexer } from "./components/indexing/directory-indexer";
import { Toaster } from "./components/ui/toaster";

function App() {
	return (
		<main className="min-h-screen bg-background">
			<div className="container py-10 space-y-8">
				<div className="flex flex-col space-y-4">
					<h1 className="text-4xl font-bold">Constella File Search</h1>
					<p className="text-muted-foreground">Fast, efficient file search for your system.</p>
				</div>

				<div className="flex flex-col space-y-6">
					<DirectoryIndexer />
					<SearchInterface />
				</div>
			</div>
			<Toaster />
		</main>
	);
}

export default App;

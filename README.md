**Native File System Search and Indexing Tool**
===============================================

**Overview**
------------

This project is a cross-platform native file system search and indexing tool inspired by modern search engines like [grep.app](https://grep.app/). Built for speed, efficiency, and extensibility, it provides instant search capabilities and a rich feature set for managing and searching through a computer's directories.

The system uses:

-   **Rust** for backend performance and file system operations.
-   **Tantivy** for high-performance indexing and search.
-   **Tauri** to bundle the backend and frontend.
-   **React** for an intuitive and feature-rich user interface.

The application is designed for **Windows**, **macOS**, and **Linux**, with compatibility for older operating systems.

* * * * *

**Features**
------------

### **Core Features**

-   **Full-Text Search**: Quickly find files and content using Tantivy-powered indexing.
-   **Advanced Query Support**: Includes regex-based search, filtering by file type, size, and modification date.
-   **Real-Time Updates**: Indexes are updated automatically as files are added, modified, or deleted.
-   **Cross-Platform**: Works seamlessly across Linux, Windows, and macOS.
-   **Optimized for Speed**: Built with performance-first principles to deliver near-instant search results.

### **Frontend (React)**

-   Dynamic, real-time search results with filtering and sorting.
-   User-friendly directory picker for configuring indexed directories.
-   Detailed file previews for supported formats (e.g., PDFs, text files).
-   Analytics and usage statistics for power users.

### **Backend (Rust)**

-   Multithreaded file system traversal for efficient indexing.
-   Tantivy-powered indexing with schema support for:
    -   File name
    -   File path
    -   File size
    -   Creation and modification dates
    -   File content
-   Secure, efficient communication with the frontend using Tauri's IPC.

* * * * *

**Folder Structure**
--------------------

The project is organized as follows:

plaintext

Copy code

`project-root/
├── src/                   # Tauri's unified source directory
│   ├── components/        # Reusable React UI components
│   ├── actions/           # API interaction logic with the Rust backend
│   ├── pages/             # Route-based pages for the React frontend
│   ├── styles/            # Global and scoped CSS
│   ├── utils/             # Frontend utility functions
│   ├── assets/            # Static assets shared across the app
│   ├── main.tsx           # Main entry point for the React app
│   ├── App.tsx            # Root React component
│   └── index.html         # HTML template for the Tauri app
├── src-tauri/             # Tauri-specific configuration and Rust backend
│   ├── commands/          # Tauri command handlers interfacing with the frontend
│   ├── indexing/          # Tantivy indexing logic and schema
│   ├── file_system/       # File system traversal and parsing logic
│   ├── api/               # API endpoints exposed to the frontend
│   ├── utils/             # Backend utility functions
│   ├── main.rs            # Entry point for the Rust backend
│   ├── Cargo.toml         # Rust dependencies
│   ├── tauri.conf.json    # Tauri configuration file
│   └── build.rs           # Optional build script for custom compilation steps
├── public/                # Public assets served statically
│   ├── favicon.ico        # Favicon
│   └── manifest.json      # Web manifest for the app
├── package.json           # Frontend dependencies
├── vite.config.ts         # Vite configuration for the project
├── README.md              # Project documentation
└── .cursorrules           # Rules for project structure and development guidelines`

* * * * *

**Getting Started**
-------------------

### **Prerequisites**

-   [Node.js](https://nodejs.org/) (for frontend development)
-   [Rust](https://www.rust-lang.org/) (for backend development)
-   Tauri prerequisites (Tauri Setup Guide)

### **Installation**

1.  Clone the repository:

    bash

    Copy code

    `git clone https://github.com/your-username/your-repo-name.git
    cd your-repo-name`

2.  Install frontend dependencies:

    bash

    Copy code

    `cd src
    npm install`

3.  Build backend dependencies:

    bash

    Copy code

    `cd ../src-tauri
    cargo build`

4.  Build the project:

    bash

    Copy code

    `tauri build`

* * * * *

**Usage**
---------

### **Running in Development Mode**

1.  Start the backend:

    bash

    Copy code

    `cd src-tauri
    cargo run`

2.  Start the frontend:

    bash

    Copy code

    `cd src
    npm run dev`

3.  Open the app: The Tauri development app will launch automatically, but you can also visit `http://localhost:3000` in your browser to view the React frontend.

### **Production Build**

To build the application as a single executable:

bash

Copy code

`tauri build`

* * * * *

**Features in Detail**
----------------------

### **File System Indexing**

-   Tantivy indexes all user-specified directories.
-   Incremental updates minimize resource usage and keep the index fresh.
-   File metadata and content are parsed for supported formats (e.g., `.txt`, `.json`, `.pdf`).

### **Advanced Search**

-   Supports full-text and regex queries.
-   Filters for file type, size, and modification date.
-   Autocomplete suggestions for frequently searched terms.

### **Secure Communication**

-   All interactions between the frontend and backend are encrypted.
-   Directories are explicitly selected by the user to prevent unauthorized access.

* * * * *

**Development Guidelines**
--------------------------

### **Strict Folder Structure**

-   **Frontend**:
    -   All components go in `src/components/`.
    -   All actions (API interactions) go in `src/actions/`.
    -   Pages go in `src/pages/`.
-   **Backend**:
    -   All indexing logic resides in `src-tauri/indexing/`.
    -   File system traversal logic resides in `src-tauri/file_system/`.
    -   API endpoints are defined in `src-tauri/api/`.

### **Codebase Iteration**

-   Regularly clean up unused files and obsolete code.
-   Temporary files must be deleted after each operation.

* * * * *

**Contributing**
----------------

Contributions are welcome! Please follow the steps below:

1.  Fork the repository.
2.  Create a new branch for your feature:

    bash

    Copy code

    `git checkout -b feature-name`

3.  Commit your changes:

    bash

    Copy code

    `git commit -m "Add your message here"`

4.  Push your branch:

    bash

    Copy code

    `git push origin feature-name`

5.  Open a pull request.

* * * * *

**License**
-----------

This project is licensed under the MIT License. See the `LICENSE` file for more details.

* * * * *

**Acknowledgments**
-------------------

-   [grep.app](https://grep.app/) for inspiring modern search concepts.
-   Tantivy for the powerful search engine.
-   [Tauri](https://tauri.app/) for the lightweight, cross-platform framework.
-   [React](https://reactjs.org/) for the robust frontend framework.
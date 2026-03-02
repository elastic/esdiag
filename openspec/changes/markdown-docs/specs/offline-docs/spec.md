## ADDED Requirements

### Requirement: Embedded Documentation Renderer
The application MUST render embedded markdown documentation to HTML on the server using the built-in markdown pipeline.

#### Scenario: Requesting a documentation page
- **WHEN** a client requests a docs route for an existing markdown document
- **THEN** the server responds with HTML generated from that markdown source.

### Requirement: Embedded Documentation Assets
The application MUST access Markdown documentation files from the `docs/` directory at the project root. This directory and its nested subdirectories MUST be accessible by the application to ensure offline availability.

#### Scenario: Requesting an existing documentation page
- **WHEN** a user navigates to a documentation path (e.g. `/docs/documentation` or `/docs/subfolder/topic`) and the file exists
- **THEN** the system serves the corresponding rendered HTML page including the Markdown content string or source.

#### Scenario: Requesting a missing documentation page
- **WHEN** a user navigates to a documentation path that does not exist
- **THEN** the system returns a 404 Not Found response or a fallback page.

### Requirement: Dynamic Table of Contents
The application MUST dynamically generate the documentation Table of Contents (TOC) by scanning the `docs/` directory at startup or compile-time, reflecting its file and folder structure.

#### Scenario: Navigating the documentation hierarchy
- **WHEN** a user accesses the documentation viewer
- **THEN** the left navigation menu displays a hierarchy of available documentation, generated from the `docs/` folder structure, including any nested subdirectories.

### Requirement: Documentation Viewer UI
The application MUST provide a dedicated user interface for viewing documentation, featuring a two-column layout with a navigation menu on the left and the rendered Markdown content on the right.

#### Scenario: Viewing the documentation index
- **WHEN** a user accesses the documentation root or index page
- **THEN** the UI displays the table of contents on the left and the rendered default documentation page in the main content area.

### Requirement: Header Navigation Link
The application Web UI header MUST include a "Book" button or link that navigates users to the documentation viewer.

#### Scenario: Clicking the header docs link
- **WHEN** a user clicks the "Book" button in the application header
- **THEN** the user is navigated to the main documentation page (`/docs/documentation` or similar).

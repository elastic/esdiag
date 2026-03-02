## Why

ESDiag needs to support fully-offline operations with no internet access. This means documentation, saved in Markdown format, needs to be viewable directly from the web interface without relying on external CDNs or network calls. This is critical for users running the tool in air-gapped or restricted environments.

## What Changes

- Add a new `/docs/{doc}` route in the Axum server to serve documentation pages.
- Parse Markdown to HTML on the server using `pulldown-cmark` with GFM-style options, then render through Askama templates.
- Add a "Book" button in the web header navigation that links to the docs page.
- Create a docs layout featuring two main sections:
  - A left navigation menu acting as a table of contents for all available documentation.
  - A main content window where the selected Markdown file is rendered from server-generated HTML.

## Capabilities

### New Capabilities
- `offline-docs`: Serving markdown documentation directly from the embedded binary, rendering it server-side via `pulldown-cmark`, and providing a UI for navigating between documentation files.

### Modified Capabilities
- `asset-compression`: Refactoring the existing asset embedding mechanism (which uses a custom tar/gz build script) to use the `rust-embed` crate with its built-in compression feature.

## Impact

- **Web Server:** A new Axum route `/docs/{doc}` will be added to serve the documentation and the associated views.
- **Web UI:** The Askama templates for the layout/header will need an update to add the "Book" button.
- **Markdown Rendering:** Markdown parsing and HTML generation moves into the server handler using `pulldown-cmark`.
- **Documentation:** Markdown documentation files will need to be embedded or loaded from a specific directory for offline access.

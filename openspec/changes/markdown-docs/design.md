## Context

Currently, ESDiag serves web pages utilizing Askama for templating and Datastar for frontend interactivity. The tool needs to be completely offline-capable, which includes the ability to view documentation without internet access. Because the documentation is in Markdown format, we need to embed a Markdown renderer and the documentation files themselves into the binary, and expose an Axum route to render them to the user.

## Goals / Non-Goals

**Goals:**
- Provide an offline documentation viewer embedded in the ESDiag Web UI.
- Serve the `marked.js` library directly from the Rust binary.
- Serve Markdown documentation files directly from the Rust binary or from the local filesystem.
- Provide a clear navigation UI (table of contents on the left, rendered content on the right).
- Include a "Book" button in the header navigation to reach the documentation.

**Non-Goals:**
- Supporting arbitrary user-provided documentation not bundled with ESDiag.
- Building a complex full-text search engine for the documentation (unless naturally provided by existing capabilities).
- Converting Markdown to HTML on the backend (using `marked.js` on the client as requested).

## Decisions

- **Embedding Assets**: 
  - `rust-embed` with the `compression` feature will be introduced to handle all embedded assets. 
  - `marked.js` and the `docs/` directory will be embedded using `rust-embed`. 
  - The existing custom `tar.gz` archive generation logic in `build.rs` will be refactored to use `rust-embed` to maintain single-binary distribution while simplifying the build process.
  - The `debug-embed` feature in `rust-embed` will be utilized in debug builds to allow live reloading of markdown files directly from the filesystem during development.
- **Serving Markdown Files**: The Markdown documentation files will also be embedded into the binary or placed in an accessible directory that is bundled with releases. The new Axum route `/docs/{doc}` will read the requested Markdown file and pass it to an Askama template.
- **Frontend Rendering**: The Askama template for `/docs/{doc}` will include a `<script type="module">` that imports the embedded `marked.js`, grabs the raw markdown (either rendered into a hidden element, or fetched from another endpoint, or passed as a JS string), and sets the `innerHTML` of the main content `div`.
- **Docs Layout**: A dedicated Askama template will be created for the docs view, establishing a two-column layout. The left column will contain a list of all available documentation files (Table of Contents), and the right column will hold the `id="content"` element for rendering the Markdown.
- **Routing**: The Axum router will be updated to handle `/docs/:doc` which returns the templated HTML page. A fallback route for `/docs` might redirect to `/docs/index` or the first available document.

## Risks / Trade-offs

- **Risk: Binary Size Increase**
  - **Mitigation:** Only necessary documentation and the minified `marked.js` will be embedded. The increase should be minimal and acceptable for a self-contained tool.
- **Risk: XSS Vulnerabilities in Markdown**
  - **Mitigation:** Since the documentation is authored by the ESDiag team and embedded at compile-time, the risk is negligible. However, if any user-provided content is rendered, `marked.js` should be configured to sanitize output or used in conjunction with DOMPurify (though not strictly necessary for static bundled docs).
- **Risk: Broken Links within Documentation**
  - **Mitigation:** Ensure that relative links within the Markdown files correctly resolve to other `/docs/{doc}` routes.
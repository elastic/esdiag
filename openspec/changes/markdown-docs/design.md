## Context

Currently, ESDiag serves web pages utilizing Askama for templating and Datastar for frontend interactivity. The tool needs to be completely offline-capable, which includes the ability to view documentation without internet access. Because the documentation is in Markdown format, we need to embed the documentation files into the binary and render Markdown on the server through an Axum route.

## Goals / Non-Goals

**Goals:**
- Provide an offline documentation viewer embedded in the ESDiag Web UI.
- Serve Markdown documentation files directly from the Rust binary or from the local filesystem.
- Render Markdown on the backend using `pulldown-cmark`.
- Provide a clear navigation UI (table of contents on the left, rendered content on the right).
- Include a "Book" button in the header navigation to reach the documentation.

**Non-Goals:**
- Supporting arbitrary user-provided documentation not bundled with ESDiag.
- Building a complex full-text search engine for the documentation (unless naturally provided by existing capabilities).
- Client-side Markdown conversion libraries or CDN-delivered rendering dependencies.

## Decisions

- **Embedding Assets**: 
  - `rust-embed` with the `compression` feature will be introduced to handle all embedded assets. 
  - The `docs/` directory will be embedded using `rust-embed`. 
  - The existing custom `tar.gz` archive generation logic in `build.rs` will be refactored to use `rust-embed` to maintain single-binary distribution while simplifying the build process.
  - The `debug-embed` feature in `rust-embed` will be utilized in debug builds to allow live reloading of markdown files directly from the filesystem during development.
- **Markdown Rendering Pipeline**: The Axum docs handler will load markdown from embedded docs, parse with `pulldown-cmark` (GFM-friendly options), and pass rendered HTML to Askama templates.
- **Frontend Rendering**: The docs template receives server-rendered HTML content and applies syntax highlighting with Prism where needed.
- **Docs Layout**: A dedicated Askama template will be created for the docs view, establishing a two-column layout. The left column will contain a list of all available documentation files (Table of Contents), and the right column will hold the `id="content"` element for rendering the Markdown.
- **Routing**: The Axum router will be updated to handle `/docs/:doc` which returns the templated HTML page. A fallback route for `/docs` might redirect to `/docs/index` or the first available document.

## Risks / Trade-offs

- **Risk: Binary Size Increase**
  - **Mitigation:** Only necessary documentation and frontend assets will be embedded. The increase should be minimal and acceptable for a self-contained tool.
- **Risk: XSS Vulnerabilities in Markdown**
  - **Mitigation:** Since the documentation is authored by the ESDiag team and embedded at compile-time, the risk is low. If user-provided markdown is ever introduced, server-side sanitization must be added before rendering.
- **Risk: Broken Links within Documentation**
  - **Mitigation:** Ensure that relative links within the Markdown files correctly resolve to other `/docs/{doc}` routes.

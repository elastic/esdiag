## 1. Asset Preparation & Refactoring

- [x] 1.1 Add backend Markdown rendering via `pulldown-cmark` with GFM-friendly parser options.
- [x] 1.2 Create a `docs/` directory at the project root and populate it with an initial `index.md` and a nested `example/subtopic.md` containing placeholder documentation.
- [x] 1.3 Add the `rust-embed` crate to `Cargo.toml` with `compression` and `debug-embed` features enabled.
- [x] 1.4 Refactor the existing `build.rs` to remove the custom `tar.gz` archive generation logic for assets.
- [x] 1.5 Implement `rust-embed` structs (`#[derive(RustEmbed)]`) to serve the `docs/` directory, `assets/` directory, and server frontend assets.
- [x] 1.6 Update any existing code that extracted the `tar.gz` archive (e.g. `setup` command or static asset serving) to use the new `rust-embed` API.

## 2. Server Routing and Navigation

- [x] 2.1 Implement docs route handling that loads embedded markdown and renders it to HTML server-side.
- [x] 2.2 Create logic to dynamically scan the `docs/` directory hierarchy (including subdirectories) to build a nested Table of Contents (TOC) structure.
- [x] 2.3 Create an Axum route handler `GET /docs/*path` (wildcard) that resolves the requested Markdown file (handling nested directories) from the `docs/` folder.
- [x] 2.4 Implement fallback logic in the handler to return a 404 response if the requested documentation file does not exist.

## 3. UI Layout and Templating

- [x] 3.1 Create a new Askama template for the documentation page featuring a two-column layout.
- [x] 3.2 Inject the dynamically generated TOC structure into the left column of the Askama template.
- [x] 3.3 Render server-generated HTML in the docs view and apply syntax highlighting behavior for code blocks.
- [x] 3.4 Update the main application header template to include a "Book" button that links to `/docs/index`.
- [x] 3.5 Wire the Askama template to the `/docs/*path` Axum handler.

## 4. Verification

- [x] 4.1 Run the web server and verify that navigating to `/docs/index` successfully loads and renders the offline documentation layout.
- [x] 4.2 Verify that navigating to a nested doc like `/docs/example/subtopic` correctly loads the content.
- [x] 4.3 Validate that the "Book" header link directs to the documentation index correctly.
- [x] 4.4 Verify that a non-existent document path properly returns a 404 response.
- [x] 4.5 Verify the dynamically generated TOC correctly displays files and subdirectories.
- [x] 4.6 Run `cargo clippy` and `cargo test` to ensure no warnings or regressions were introduced.

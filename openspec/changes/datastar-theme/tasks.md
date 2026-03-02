## 1. Theme Model and Persistence

- [x] 1.1 Add a Web UI theme context model constrained to `borealis` with light/dark mode state.
- [x] 1.2 Implement cookie parsing/normalization for theme mode override (`theme_dark`) with a 1-year lifetime.
- [x] 1.3 Implement default mode behavior from operating system preference when no cookie override exists.
- [x] 1.4 Add shared helpers so full-page and Datastar fragment responses use the same theme state source.

## 2. Theme Route and Datastar Patching

- [x] 2.1 Add `POST /theme` route to persist dark-mode toggle state.
- [x] 2.2 Implement Datastar patch response to update dark-mode signals and toggle state without full page reload.
- [x] 2.3 Set persistence cookies from the theme route using `Path=/`, `Max-Age=31536000`, and `SameSite=Lax`.
- [x] 2.4 Keep compatibility with existing docs partial update flow and other Datastar-driven UI actions.

## 3. Assets and Embedding

- [x] 3.1 Copy `dsuite` borealis token stylesheet into server web assets as the ESDiag borealis theme base.
- [x] 3.2 Merge required `dsuite` base style primitives into the main application stylesheet while preserving ESDiag layout/components.
- [x] 3.3 Add theme stylesheet serving/embedding through existing `rust-embed` web asset pipeline.
- [x] 3.4 Ensure no external CSS/JS dependencies are introduced for theming behavior.

## 4. Template and Header Integration

- [x] 4.1 Update shared layout template to include `#theme-stylesheet` and dark-mode binding infrastructure.
- [x] 4.2 Add header dark-mode toggle control for all users/routes.
- [x] 4.3 Wire toggle control to Datastar theme route actions and keep signal state stable across navigation.
- [x] 4.4 Ensure docs and non-doc pages inherit the same global theme behavior through shared templates.

## 5. Full Token Migration and Docs/Prism Theming

- [x] 5.1 Refactor existing hard-coded color usage to semantic theme tokens across all current pages/components.
- [x] 5.2 Retheme markdown/docs surfaces (content, nav, links, callouts, code containers) to the borealis palette via tokens.
- [x] 5.3 Select and apply a temporary default dark Prism theme compatible with borealis dark mode.
- [x] 5.4 Eliminate newly introduced hard-coded color values in favor of token references for this change.

## 6. Verification

- [x] 6.1 Verify manual behavior: first-load OS preference, user mode toggle override, cookie persistence across reload/routes.
- [x] 6.2 Verify global coverage: index/upload flow, docs routes (`/docs`, `/docs/*path`), header/footer, and Datastar partial updates.
- [x] 6.3 Verify offline behavior by running without network access and confirming themed assets load from embedded sources.
- [x] 6.4 Run `cargo clippy` and `cargo test` and resolve any regressions introduced by the theming changes.

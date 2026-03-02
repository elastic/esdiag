## Context

ESDiag already serves a Datastar-powered Web UI through Axum + Askama with shared layout/templates and embedded frontend assets. Styling is currently centralized in a single stylesheet with hardcoded values and no first-class theme model, which makes broad visual updates and consistent light/dark behavior difficult.

The new `ui-theming` capability introduces application-wide theming, not limited to the docs page. The `~/Development/dsuite` repository provides a proven reference for:
- separating base component/layout CSS from theme token files,
- toggling dark mode with Datastar state,
- persisting user preference via cookies,
- patching UI mode state without full-page reload.

Constraints:
- Must remain fully offline and self-contained.
- Must work across all existing routes/components that share `templates/layout.html` and server-rendered fragments.
- Must not break current Datastar partial updates for docs and stats.

## Goals / Non-Goals

**Goals:**
- Add a global theme model with light/dark support across the whole UI.
- Persist dark mode across navigation/reloads.
- Provide a header control for mode toggle.
- Refactor styling toward design tokens to reduce hardcoded color usage.
- Keep compatibility with embedded assets and offline operation.

**Non-Goals:**
- Rebuilding all page layouts or introducing a new component framework.
- User-defined custom themes loaded from arbitrary files.
- Runtime downloads of external CSS or JS resources.
- Modifying CLI/runtime processing behavior unrelated to Web UI rendering.

## Decisions

### 1) Theme State Is Server-Backed and Cookie-Persisted
- Add a `ThemeContext` (dark bool) parsed from the `Cookie` header.
- Default values are provided server-side so first load is deterministic.
- Dark-mode preference is persisted via `Set-Cookie` headers on mode update requests.

Rationale:
- Works for full page loads and Datastar fragment requests consistently.
- Requires no localStorage dependency and stays compatible with strict environments.

Alternative considered:
- `localStorage`-only state. Rejected because server-rendered responses would not know active theme at render time.

### 2) Dedicated Mode Update Route with Datastar Patches
- Add `POST /theme` route that accepts Datastar signals (dark mode flag).
- Route returns Datastar patch events to:
  - update theme signals (`theme.dark`),
  - keep toggle state in sync with cookie-backed server state.

Rationale:
- Mirrors existing Datastar interaction model.
- Avoids full reload when changing mode.

Alternative considered:
- full-page redirect after selecting theme. Rejected due to poorer UX and unnecessary rerender cost.

### 3) CSS Architecture: Base Styles + Borealis Token Stylesheet
- Split styling into:
  - base/shared stylesheet (`style.css`) for layout/components/semantics,
  - Borealis token stylesheet defining CSS variables for light/dark values.
- Base styles consume semantic variables only (e.g., `--text-color`, `--border-color`, `--background-color`).
- Dark mode toggles Borealis variable sets, following the `dsuite` approach (`:has(#dark-mode:checked)` pattern).

Rationale:
- Keeps implementation simple for single-theme support.
- Enables broad visual refresh with lower risk of regressions.

Alternative considered:
- duplicate full CSS for light and dark modes. Rejected as high-maintenance and error-prone.

### 4) Shared Layout Owns Theme Bootstrapping
- Update shared layout/header templates to include:
  - theme stylesheet link (`id="theme-stylesheet"`),
  - hidden dark-mode control bound to Datastar signals,
  - header dark-mode toggle control.
- Ensure all pages that use shared layout inherit the same theme behavior by default.

Rationale:
- Single integration point ensures consistent behavior across docs and non-doc pages.

Alternative considered:
- page-by-page theme integration. Rejected due to drift risk and duplicated logic.

### 5) Compatibility with Existing Docs/DataStar Flows
- Keep current docs partial-render flow (`/docs/*path`) intact.
- Theme signals/state live at document scope so docs fragment updates do not reset theme.
- Prism and existing scripts continue to run against themed surfaces.

Rationale:
- Avoids regressions in already working markdown/docs behavior while expanding scope.

## Risks / Trade-offs

- [Risk: Browser support for advanced selectors like `:has`] -> Mitigation: verify target browser support; if needed, add fallback class-based dark-mode toggling via patched `data-theme` attribute.
- [Risk: Incomplete variable migration leaves mixed old/new colors] -> Mitigation: migrate in defined passes and include visual verification checklist per route/component.
- [Risk: Contrast regressions in dark mode] -> Mitigation: include explicit contrast checks for text, links, code blocks, alerts, and interactive controls.
- [Risk: Theme state desync between cookie and Datastar signals] -> Mitigation: always patch signals from server response and treat server cookie as source of truth on next request.
- [Trade-off: Added CSS files/routes increase complexity] -> Mitigation: keep theme API narrow (`dark` only) and centralize parsing/normalization.

## Migration Plan

1. Introduce theme model and parser utilities (OS-preference default + cookie override parsing/normalization).
2. Add mode route (`POST /theme`) returning Datastar patch events and `Set-Cookie` headers.
3. Add theme asset files and update server asset routing/embedding for theme stylesheets.
4. Update shared layout/header templates for theme stylesheet link and mode toggle.
5. Refactor base CSS to semantic variables and migrate key components/pages incrementally.
6. Verify across docs, index/upload flows, and shared components in both light/dark mode.
7. Rollback strategy: remove theme route/template controls and fallback to existing single stylesheet/default look.

## Open Questions

- None.

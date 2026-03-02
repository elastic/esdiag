## Why

The current Web UI styling is largely static and does not support user-selectable light/dark themes across pages. We need a unified theming model now to support the markdown/docs work and to keep the broader Datastar-based UI visually consistent, accessible, and maintainable.

## What Changes

- Introduce a global Borealis theme system for the full Web UI (not only the docs viewer), including light/dark mode support.
- Add theme mode state management backed by request/cookie context so dark-mode preference persists across routes and reloads.
- Add a Datastar-driven UI control in the shared header for light/dark toggle.
- Refactor shared CSS to use design tokens/custom properties, with Borealis tokens adapted from the `~/Development/dsuite` approach.
- Ensure all major pages and shared components (header, footer, forms, docs viewer, status components, nav elements) consume the new theme tokens.
- Add backend route support for theme updates (Datastar patch/event flow) and persistence behavior.
- Preserve offline/self-contained operation (no external theme/CDN dependencies).

## Capabilities

### New Capabilities
- `ui-theming`: Provide an application-wide Borealis theming system for the Axum/Askama/Datastar Web UI, including persisted light/dark mode.

### Modified Capabilities
- None.

## Impact

- **Target product:** Elasticsearch diagnostics web interface in ESDiag.
- **Affected area:** Web UI primarily (Askama templates, Datastar interactions, CSS assets, Axum routes); minimal core processing impact.
- **Server:** Add/adjust route(s) and request context handling for dark-mode updates and persisted preferences.
- **Frontend:** Shared layout/header and component templates require updates for dark-mode controls and tokenized styling.
- **Assets:** Add Borealis token CSS and reorganize base style variables.
- **Testing/verification:** Expand UI verification to include theme persistence, route transitions, and readability/contrast across major screens.

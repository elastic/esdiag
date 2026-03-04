## Why

With desktop distribution now available through Tauri, the web interface needs two explicit runtime modes with different trust and persistence assumptions. Defining `service` and `user` modes now prevents configuration drift, avoids invalid persistence behavior in ephemeral service containers, and preserves the current CLI behavior.

## What Changes

- Add explicit web runtime modes: `service` and `user`, applied to both `serve` and desktop-hosted web interfaces.
- Introduce mode-aware authentication and state behavior for the web backend (IAP header trust in `service`, local credential flow in `user`).
- Make settings and local artifact persistence mode-dependent (skip `hosts.yml`/similar writes in `service`; keep full local persistence in `user`).
- Split exporter behavior by mode (startup-defined in `service`, runtime-configurable in `user`).
- Keep CLI collection and processing behavior unchanged.

## Capabilities

### New Capabilities
- `web-runtime-modes`: Defines `service` and `user` runtime contracts for the web interface, including auth source, persistence boundaries, preference scope, and exporter mutability.

### Modified Capabilities
- `tauri-desktop-app`: Clarify that mode handling applies to desktop-hosted web UI behavior without changing standalone CLI lifecycles.
- `desktop-settings`: Add mode-gated settings behavior so `service` exposes limited preferences and avoids local credential/host persistence, while `user` retains rich configurable settings.

## Impact

- Affected systems: Axum web state/bootstrap, web auth middleware, settings/host persistence services, exporter selection flow, and Tauri startup wiring.
- Affected interfaces: Web UI for `serve` and desktop app variants only.
- Compatibility: CLI contract remains unchanged; no new CLI requirements.

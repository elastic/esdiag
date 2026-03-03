## 1. Backend Settings Configuration

- [x] 1.1 Define a `Settings` struct (containing `active_target` and `kibana_url`) mapped via Serde.
- [x] 1.2 Implement utility functions in `src/data/` or a new `src/settings.rs` to read/write `~/.esdiag/settings.yml` securely.
- [x] 1.3 Update `src/main.rs` to load the settings if no CLI arguments are supplied, applying the loaded config to `Server::start`.

## 2. Server State Management

- [x] 2.1 Update the `ServerState` struct in `src/server/mod.rs` to wrap its `Exporter` in a `RwLock` or `Mutex` so it can be swapped.
- [x] 2.2 Create a new Axum API endpoint `GET /api/settings` to fetch the current active settings and a list of all available `KnownHost` names.
- [x] 2.3 Create a new Axum API endpoint `POST /api/settings/update` to receive a new Target / Kibana configuration, test the connection (if new host), update `ServerState`, and write to `settings.yml`.

## 3. Frontend Settings Modal

- [x] 3.1 Add a new `SettingsModal` Askama template representing the settings form.
- [x] 3.2 Add a clickable "Target" status indicator in the web footer (`src/server/template.rs`) that triggers a Datastar request to fetch the modal.
- [x] 3.3 Ensure password/API key inputs in the template use `type="password"`.
- [x] 3.4 Add an endpoint `GET /settings/modal` to render and return the `SettingsModal` HTML snippet via Datastar.

## 4. Verification & Polish

- [x] 4.1 Test starting the application without flags to confirm it loads defaults from `settings.yml`.
- [x] 4.2 Test modifying the Kibana URL and Target Host via the UI and verify `settings.yml` is updated immediately.
- [x] 4.3 Run `cargo clippy` and `cargo test` to ensure trait adherence and lack of regressions.

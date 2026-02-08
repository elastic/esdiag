## 1. Model Updates

- [x] 1.1 Update `Asset` struct to include `requires_security` field.
- [x] 1.2 Update YAML parsing logic for `assets.yml` to handle the new field.

## 2. Security Detection

- [x] 2.1 Implement a utility function to check Elasticsearch security status.
- [x] 2.2 Integrate the security check into the setup/diagnostic initialization.

## 3. Asset Filtering

- [x] 3.1 Implement filtering logic to remove security-dependent assets when security is disabled.
- [x] 3.2 Update `src/setup.rs` to apply this filter during asset loading.

## 4. Asset Configuration Updates

- [x] 4.1 Update Elasticsearch `assets.yml` to mark `_security` assets with `requires_security: true`.
- [x] 4.2 Review other product assets (e.g., Kibana) for security dependencies.

## 5. Verification

- [x] 5.1 Run `cargo clippy` to ensure code quality.
- [x] 5.2 Run `cargo test` to verify functionality.
- [x] 5.3 Verify skipping logic manually against a cluster with security disabled.

## Why

Some Elasticsearch API endpoints (like `_security`) are rejected or do not exist when security is disabled. Currently, `esdiag` attempts to fetch these assets regardless, resulting in logged errors that are expected but clutter the diagnostic output and may mislead users into thinking something is broken.

## What Changes

- **Update Asset Configuration**: Modify the `assets.yml` structure to allow specifying if an asset depends on security being enabled.
- **Security Awareness**: Implement a check to determine if the target Elasticsearch cluster has security enabled.
- **Conditional Asset Loading**: Update the asset processing logic to skip security-dependent assets when security is disabled on the cluster.

## Capabilities

### New Capabilities
- `asset-security-filtering`: Logic and configuration for conditionally skipping diagnostic assets based on the cluster's security configuration.

### Modified Capabilities
- None.

## Impact

- `src/setup.rs`: Asset parsing and loading logic.
- Diagnostic collection orchestration: Logic to skip assets during the gathering phase.
- `assets.yml` files: Update schema for various products (Elasticsearch, Kibana).

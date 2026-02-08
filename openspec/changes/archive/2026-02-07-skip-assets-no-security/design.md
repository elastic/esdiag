## Context

`esdiag` collects diagnostic assets based on product-specific `assets.yml` files. These files list API endpoints and resources to be gathered. Currently, the collection process is unconditional—if an asset is listed, the system attempts to collect it. For Elasticsearch clusters where security is disabled, endpoints like `/_security` return errors, leading to unnecessary noise in the diagnostic logs.

## Goals / Non-Goals

**Goals:**
- Provide a way to mark assets as dependent on Elasticsearch security.
- Detect the security state of the target Elasticsearch cluster during the initial connection/setup.
- Prevent attempts to collect security-dependent assets when security is disabled.

**Non-Goals:**
- Implementing a complex dependency management system for assets.
- Modifying the core `Receiver` or `Processor` traits.

## Decisions

### 1. Extend Asset Schema
The `Asset` struct (and the corresponding `assets.yml` parser) will be extended to include a `requires_security` boolean field.
- **Rationale**: This is the most direct way to encode the requirement in the existing configuration format.
- **Alternatives**: Using a separate file for security-dependent assets, but this would lead to fragmentation and duplicated logic for other fields.

### 2. Early Security Detection
The system will check the security status of the cluster during the `setup` phase. This can be achieved by checking `_xpack/usage`.
- **Robustness**: 
  - Status codes 401/403 (Unauthorized/Forbidden) indicate security IS enabled, but access to the usage API is restricted.
  - Status code 404 (Not Found) indicates the endpoint is missing, likely because security is disabled or not supported.
  - Other non-success status codes or network errors will result in a hard failure to avoid silent misconfiguration and missing diagnostic data.
- **Rationale**: Detecting this once at the start is more efficient than checking before every asset collection.
- **Alternatives**: Check security status "on-demand" for each asset, but this adds latency and complexity to the collection loop.

### 3. Filter Assets at Source
The asset list will be filtered in `src/setup.rs` immediately after parsing `assets.yml`.
- **Rationale**: By filtering early, the rest of the orchestration and collection logic remains unchanged and doesn't need to be "security aware".
- **Alternatives**: Pass the security status down to the `Receiver`, but this would require changing trait signatures and updating all receiver implementations.

## Risks / Trade-offs

- **[Risk] Detection Failure** → If the security detection check fails (e.g., due to connectivity), the system might incorrectly skip assets or try to collect ones it shouldn't. 
  - **Mitigation**: Fail fast on ambiguous errors. Map 401/403 to `security: true` and 404 to `security: false`.
- **[Trade-off] Performance** → Adding an extra API call during setup.
  - **Mitigation**: The impact is negligible compared to the overall diagnostic collection time.

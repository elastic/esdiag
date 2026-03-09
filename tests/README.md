# External Collection Tests

The ignored tests in `tests/logstash_collection_tests.rs` validate Logstash support collection
against real external services. They are intentionally ignored because they depend on
environment-specific infrastructure.

## Required Environment Variables

Set `*_URL` for each version you want to exercise:

- `ESDIAG_LOGSTASH_68_URL`
- `ESDIAG_LOGSTASH_717_URL`
- `ESDIAG_LOGSTASH_819_URL`
- `ESDIAG_LOGSTASH_9_URL`

Authentication is optional and can be provided per target with either:

- `*_APIKEY`
- `*_USERNAME` and `*_PASSWORD`

If the target uses a self-signed or otherwise invalid TLS certificate, set:

- `*_ACCEPT_INVALID_CERTS=true`

Examples:

- `ESDIAG_LOGSTASH_68_URL=https://ls68.example.org:9600`
- `ESDIAG_LOGSTASH_68_USERNAME=elastic`
- `ESDIAG_LOGSTASH_68_PASSWORD=changeme`
- `ESDIAG_LOGSTASH_68_ACCEPT_INVALID_CERTS=true`

Run them with:

```sh
cargo test --test logstash_collection_tests -- --ignored
```

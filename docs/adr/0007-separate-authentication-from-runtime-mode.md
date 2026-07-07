---
status: accepted
---

# Runtime mode bundles tenancy and capability; authentication is a separate axis

`RuntimeMode` (`User` = single-user local, `Service` = multi-user hosted) **keeps
tenancy and capability bundled on purpose** — multi-tenancy necessitates the
lockdown, so `Service` correctly forbids a shared keystore, user-editable exporter,
and host management, and forces all processed diagnostics to the one shared cluster.
But **authentication is separated out of the mode enum** into a pluggable,
provider-agnostic concern, because auth varies independently of tenancy.

## Considered options

- **Keep auth welded to mode** (`Service` ⟺ require Google IAP header, `User` ⟺
  none). Rejected:
  1. `Service` is untestable locally without a Google IAP — the header has to be
     injected by hand with a browser plugin.
  2. Self-managed deployments may front the service with a *different* IAP provider.
  3. A future `User` mode may authenticate via Elastic Cloud SSO to `Send`
     diagnostics to the support portal — auth in single-user mode.
  4. Authenticated identity should populate `Identifiers` (user + account) on
     bundles even in single-user mode.
- **Separate, pluggable authentication (chosen).** Auth is a provider abstraction
  (Google IAP, other IAP, Cloud SSO, or none) configured independently of mode.

## Consequences

- **Tenancy still drives capability** — that coupling is retained deliberately; only
  auth is unbundled.
- **`Service` can run without IAP** (local testing) or behind a non-Google provider;
  the current `requires_iap_headers()` gate becomes "which auth provider," not
  "is this Service mode."
- **Authentication serves two purposes**, not just one: *access control* (gate a
  shared service) and *identity provenance* — populating `Identifiers` and
  authorizing outbound `Send` to the support portal (a future single-user
  enhancement via Cloud SSO).
- Authenticated identity flows into `Identifiers` (user, account) attached to
  bundles, regardless of mode.

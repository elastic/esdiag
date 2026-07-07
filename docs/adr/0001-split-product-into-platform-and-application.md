---
status: accepted
---

# Split the flat `Product` enum into orthogonal `Platform` and `Application` axes

A diagnostic is classified on two independent axes that today's single `Product`
enum flattens together: the **platform** it was collected from (the deployment
environment) and the **application** whose data it carries (the Stack component).
We are replacing `Product` with a required `Platform` and an optional `Application`
because the two are genuinely orthogonal — an Elasticsearch diagnostic on ECK needs
*both* populated, a self-managed one needs only the application plus opt-in host
syscalls, and an ECE diagnostic has only the platform — and the flat enum can
express none of these combinations.

## Considered options

- **Keep the flat `Product` enum.** Rejected: it forces a single choice where two
  orthogonal facts exist, cannot represent Elasticsearch-on-ECK, and leaves
  `SelfManaged` unrepresentable (it is implicit today as "an application with no
  platform wrapper").
- **Two orthogonal fields (chosen).** `Platform` (required, mutually exclusive,
  total) and `Application` (optional).

## Consequences

- **`Application` is a closed set of the four Stack components** (`Elasticsearch |
  Kibana | Logstash | Agent`) and never holds a platform value. A platform's own
  data — e.g. the `ECK`/`ECE` orchestration diagnostic — is `application: None`,
  not `application: ECK`; encoding the platform in both axes would re-collapse the
  orthogonality this split exists to create. One ECK bundle therefore yields a
  root diagnostic (`platform: ECK, application: None`) that *includes* the
  `Elasticsearch` and `Kibana` application diagnostics — a parent plus children,
  not three peers. Display label / ID derives from `application` if present, else
  `platform`, so no fake application variant is needed to name the root.
- **`Platform` is total and required.** Every diagnostic has exactly one:
  `SelfManaged | ElasticCloudHosted | ECE | ECK | KubernetesPlatform | Unknown`.
  There is no "no platform" case — a bare install is `SelfManaged`. `Unknown` is
  the escape hatch for indeterminate provenance.
- **Platform is determined best-effort at the receiver.** ESDiag infers it from
  indicators (a `syscalls` folder implies `SelfManaged`, a manifest `runner` of
  `ece` implies `ECE`, etc.). Inference is not guaranteed — legacy
  `support-diagnostics` bundles may leave it `Unknown` — so every receiver must set
  it where it can, and downstream code must tolerate `Unknown`.
- **Platform does not propagate inherently.** When ESDiag processes a platform
  diagnostic's `included_diagnostics`, it sets the `platform` on each child
  diagnostic job as it spawns it. Included diagnostics are always applications,
  never another platform (inclusion is one level deep and layer-homogeneous).
- **Infrastructure data is dispatched off the platform, not a third axis.**
  `SelfManaged` yields opt-in Linux syscalls (available under no other platform),
  `ECK` yields container metrics, managed cloud yields none.
- **Migration touches every `Product` call site** (~90), the manifest schema, and
  the receiver/collector wiring. `Product` remains only as a legacy alias during
  the transition and takes no new concepts.

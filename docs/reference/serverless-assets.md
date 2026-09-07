---
type: Reference
title: Serverless asset compatibility
description: Elasticsearch and Kibana setup asset audit and live verification.
tags: [setup, serverless, elasticsearch, kibana]
---

# Serverless asset compatibility

Audited for issue [#390](https://github.com/elastic/esdiag/issues/390) on
2026-09-04 using the saved `esdiag-less` and `esdiag-less-kb` hosts. Both
products reported version `9.6.0` and build flavor `serverless`.

Setup reads `version.build_flavor` from Elasticsearch's root response or
Kibana's `/api/status`. It does not infer deployment type from the hostname.
An Elasticsearch security usage response of HTTP 410 reports security as
enabled. Deployment metadata selects the Serverless asset adaptations;
security status does not determine deployment compatibility.

## Findings and changes

| Item | Finding | Setup behavior |
| --- | --- | --- |
| Security usage | `/_xpack/usage` returns HTTP 410; Serverless always has security enabled | Skip the probe on a known Serverless deployment and report security as enabled for HTTP 410. |
| Bundled role | The stateful role asset is not provisioned on Serverless | Skip security-dependent assets and tell the administrator to configure project roles separately. Authentication remains enabled. |
| `esdiag@settings` | `index.lifecycle.prefer_ilm` and `index.lifecycle.name` are unsupported ILM settings | Remove these settings from outgoing Serverless templates. Keep stateful assets unchanged. |
| Retention | Data stream lifecycle is supported | Preserve `template.lifecycle.data_retention: 30d`. |
| Kibana space | The project rejects `solution` and a nonempty `disabledFeatures` list | Omit both controls for Serverless. Preserve the space ID, name, description, and appearance. |
| Trial license | Serverless manages feature entitlements | Skip `/_license` and `/_license/start_trial`; Kibana APIs still report missing feature access or privileges. |
| Default agent | A GET response includes fields that the update API rejects, including `access_control.entries` | Send only the existing configuration with the ESDiag skill added. Preserve existing skill IDs and avoid duplicates. |

The remaining template settings are `index.codec`, `index.mapping.source.mode`,
`index.mapping.ignore_malformed`, `index.mapping.total_fields.limit`,
`index.mapping.total_fields.ignore_dynamic_beyond_limit`, and
`index.query.default_field`. All appear in Elastic's
[Serverless settings list](https://www.elastic.co/docs/reference/elasticsearch/index-settings/serverless).
The regression test inventories settings across every component and index
template and requires a compatibility review when that list grows.

The agent update follows the
[partial update API](https://www.elastic.co/docs/api/doc/kibana/operation/operation-put-agent-builder-agents-id).
It does not modify the agent's access controls.

## Retention

The shared `esdiag@settings` component configures
`template.lifecycle.data_retention: 30d`, including on Serverless. Removing
unsupported ILM settings preserves this data stream lifecycle configuration.

On stateful deployments, diagnostic reports in `metrics-diagnostic-esdiag`
are retained indefinitely. Their index template disables data stream lifecycle
and clears the inherited retention. Serverless rejects a disabled lifecycle,
so setup keeps it enabled with `data_retention: 3650d` for reports. This is a
requested 10-year retention, subject to project retention limits.

Template lifecycle changes apply to newly created data streams. To apply the
Serverless report policy to an existing stream, run in Kibana Dev Tools:

```http
PUT /_data_stream/metrics-diagnostic-esdiag/_lifecycle
{"enabled": true, "data_retention": "3650d"}
```

## Indexing failures and recovery

Setup enables the failure store through the shared `esdiag@settings` component.
New ESDiag streams retain documents rejected by mappings or ingest pipelines.
These documents still count toward `documents_failed` and the diagnostic's
partial or failed outcome, even when Elasticsearch returns HTTP 201 with
`failure_store: used`.

The shared template sets `index.codec: best_compression` for regular backing
indices. Elasticsearch currently filters this setting out when creating failure
indices; its [failure-store settings allowlist](https://github.com/elastic/elasticsearch/blob/main/server/src/main/java/org/elasticsearch/cluster/metadata/DataStreamFailureStoreDefinition.java)
does not include `index.codec`. A live Serverless check with an isolated template
confirmed `best_compression` on the regular backing index and `default` on a newly
created failure index. Template configuration therefore cannot currently enforce
failure-store compression. Recheck this capability when Elasticsearch adds support;
do not infer failure-index settings from template simulation alone.

Existing streams need a separate options update. Template updates and rollover
do not change their failure-store options:

```http
PUT /_data_stream/*-esdiag/_options
{"failure_store": {"enabled": true}}

GET /health-impact-esdiag::failures/_search
```

Inspect `document.source` for the rejected document and `error` for its cause.
Failure-store retention is separate from the diagnostic stream's retention.
The [failure store documentation](https://www.elastic.co/docs/manage-data/data-store/data-streams/failure-store)
describes retention and the required `read_failure_store` and
`manage_failure_store` privileges. Configure these privileges in Serverless
project roles where needed.

Indexing failures name the destination returned for each bulk item, including
destinations selected by the ingest pipeline. ESDiag reports the stream name
for its `.ds-` and `.fs-` backing indices. Request failures with no item
destination retain the submitted stream name.

The result card and persisted report include up to three distinct rejection
reason samples per destination, truncated to 1024 characters each. If a bulk
response only reports failure-store capture, the warning identifies the
`::failures` stream to inspect.

Cluster settings can contain both `rest.incremental_bulk` and
`rest.incremental_bulk.request_timeout`. The cluster-settings template keeps
dots literal beneath `rest` with `subobjects: false`, preserving both values
without renaming settings. This follows the same mapping rule as the existing
watermark and logger namespaces: disable subobject expansion at the namespace
that permits value and sub-setting keys. Existing cluster-settings streams
need a rollover after setup before replaying affected diagnostics:

```http
POST /settings-cluster-esdiag/_rollover
```

Node settings use the same rule beneath `node.settings.http` and
`node.settings.transport`, preserving both `type` and `type.default`.
Existing node-settings streams also need `POST /settings-node-esdiag/_rollover`
after setup.

The node-settings template explicitly maps `http.max_warning_header_size` as a
keyword. Dynamic templates apply only to unmapped fields, so this prevents the
human-readable size rule from creating a disabled object beneath the flattened
HTTP namespace. The shared suppression rules need no path exclusions.

Dashboard links remain references to the shared navigation objects. Obsolete
`embeddableConfig.savedObjectId` fields and duplicate references are removed:
current Kibana transforms the saved-object reference into `ref_id`, and its
strict schema rejects the obsolete field. Both reference and inline links
panels remain supported.

Live checks against the `serverless-dev` CLI context indexed all 490 documents
from a regression fixture and all 284 documents from a fresh local collection.
The fixture includes scalar settings with dotted sub-settings and health
impact and diagnosis records. Tests inspect deployment flavor and API behavior;
they do not require a fixed Serverless `version.number`.
All 12 embedded dashboard searches also completed without shard failures.
Vega specs use strict JSON so the audit can parse their queries instead of
silently skipping them.

The complete archive matrix indexed 1,310 documents from all four Elasticsearch
and four Logstash bundles without rejections. Kibana archive processing is not
implemented; all four Kibana bundles stop before export. A live stateful-source
run indexed 3,532 documents without rejections after the node-setting fix. Its
partial outcome reflected an optional searchable-snapshot statistics request
returning HTTP 404 because the source had no searchable snapshot indices.

An isolated live rejection test also verifies pipeline routing, attribution to
impact and diagnosis streams, failure-store recovery, and direct rejection
reasons with capture disabled. Its UUID-named streams and templates are removed
after the assertions; shared diagnostic streams are not modified.

An Elasticsearch asset rejection is recorded in the setup report's `warnings`.
The combined `esdiag setup` command continues to Kibana and reports a partial
outcome if asset installation or mapping updates remain incomplete. Errors
that prevent probing a deployment, or a separate Kibana setup error, still
fail the command.

## Asset inventory

| Asset family | Count | Audit scope |
| --- | ---: | --- |
| Elasticsearch ingest pipelines | 1 | `set` processors and `reroute`; installation and simulation |
| Elasticsearch component templates | 7 | Settings, mappings, metadata, and composition |
| Elasticsearch index templates | 26 | Settings, mappings, data streams, and composition |
| Elasticsearch roles | 1 | Skipped on Serverless |
| Kibana spaces | 1 | Creation and update with project-managed controls omitted |
| Kibana saved objects | 90 | Dashboards, data views, Lens, saved searches, Vega visualizations, links, and tags |
| Kibana workflows | 1 | Installation and its Elasticsearch authentication and ES|QL request definitions |
| Agent Builder tools | 1 | Workflow reference and installation |
| Agent Builder skills | 1 | Installation, referenced diagnostic guides, and attachment to the default agent |
| Custom Agent Builder agents | 0 | The bundle extends the built-in agent |

ILM, SLM, allocation, node, and shard names inside diagnostic mappings and
dashboard queries describe imported source data. They do not configure those
features on the destination. The audit retains them, including the lifecycle
dashboards and data views. No bundled query uses the unsupported
`scripted_metric` aggregation, and the templates do not define join fields.
See Elastic's [Hosted and Serverless comparison](https://www.elastic.co/docs/deploy-manage/deploy/elastic-cloud/differences-from-other-elasticsearch-offerings).

The `sources.yml` files define diagnostic **collection** requests, including
stateful APIs such as ILM and node statistics. Setup does not send these
requests. This audit covers Serverless as the diagnostic destination and
viewer; it does not establish Serverless diagnostic collection support.

## Repeat the verification

Both setup commands completed successfully on the audited project. The live
audit passed after reading all 34 installed Elasticsearch assets, simulating
all 26 composed index templates and the pipeline, finding all 90 Kibana saved
objects, verifying their references, and checking the workflow, tool, skill,
and default-agent attachment. Embedded Vega searches also succeeded.

Kibana may assign new IDs when an imported object already exists in another
space. The audit matches these objects by `originId` and checks their actual
references; it does not assume every imported object retains its original ID.
Diagnostic links likewise resolve the imported Cluster Report and Node Settings
data-view IDs in the output's Kibana space. If the viewer cannot be read or the
required objects are absent, processing omits the link and logs the reason.

The `serverless-dev` audit also completed Elasticsearch and Kibana setup,
verified all 90 saved objects and their references, checked default-agent skill
attachment, and confirmed navigation panels survive Kibana's dashboard API
transformation. That API omits unsupported `legacy_vis` panels from its response;
this is an API limitation and does not establish browser rendering behavior.

Use an unlocked keystore and saved Elasticsearch and Kibana hosts for a
Serverless test project. These setup commands install or update ESDiag assets:

```sh
cargo build --bin esdiag
target/debug/esdiag setup esdiag-less
target/debug/esdiag setup esdiag-less-kb
```

The explicit live test reads installed assets, simulates composed templates
and the ingest pipeline, checks Kibana objects and default-agent skill
attachment, and executes embedded Vega searches. It does not create
diagnostic data or run an agent conversation:

```sh
ESDIAG_SERVERLESS_TEST_HOST=esdiag-less \
ESDIAG_SERVERLESS_TEST_KIBANA_HOST=esdiag-less-kb \
cargo test --lib setup::serverless_tests::live_serverless_asset_audit -- --ignored --nocapture
```

Offline checks:

```sh
cargo test --lib setup::
cargo test --test serverless_setup_tests
cargo test --test elasticsearch_asset_templates_tests
cargo test --test elasticsearch_feedback_tests
```

With `ESDIAG_OUTPUT_*` set for a Serverless test project, run the Elasticsearch
setup and isolated rejection checks explicitly:

```sh
cargo test --test elasticsearch_feedback_tests live_serverless_ -- --ignored --nocapture
cargo test --lib live_serverless_dashboard_searches -- --ignored --nocapture
```

The hosted importer replaces the existing upload card when processing starts.
Warnings on the completed card include rejection reason samples from the report.

Live validation covers API acceptance and asset composition. Dashboard
rendering, every ES|QL query against representative diagnostic data, and
workflow execution under an interactive user's identity need separate
functional testing. Agent Builder availability depends on project feature
tiers and privileges, as described in the
[Agent Builder setup guide](https://www.elastic.co/docs/explore-analyze/ai-features/agent-builder/get-started).

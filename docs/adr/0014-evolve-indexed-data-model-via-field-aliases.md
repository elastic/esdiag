---
type: Reference
title: "Evolve indexed provenance with writable fields and historical aliases"
status: accepted
tags: [repository, adr]
---

# Evolve indexed provenance with writable fields and historical aliases

ADR-0001 replaces `diagnostic.product` with `diagnostic.application` and
`diagnostic.orchestration` with `diagnostic.platform`. Both writer generations
must coexist on shared diagnostics clusters, including after rollover.

## Context

ESDiag controls templates installed by `setup`, but templates only affect new
indices. The original alias migration preserved searches while rejecting writes
from released 0.16.x writers on new indices. Elasticsearch field aliases cannot
accept values.

## Decisions

- New indices map all four names as concrete keyword fields. Each pair uses
  reciprocal `copy_to`, so documents written with either name are searchable and
  aggregatable through both names. This covers report, Elasticsearch, and
  Logstash metadata templates.
- Writers keep emitting their own field names. No writer upgrade is required
  before installing these templates. Copying happens in the mapping, including
  for direct report writes and bulk requests without an ingest pipeline.
- `copy_to` copies indexed values without changing ordinary `_source`. It does
  not recurse. If a document supplies different values for both names, both
  fields index both values. Writers should send one name per pair or agreeing
  values. Synthetic source may expose the indexed copies.
- Historical indices retain their concrete fields. `setup` still adds the
  current name as a query-only alias when only the legacy name exists, preserving
  searches over documents already stored there.
- `setup` warns about existing aliases that reject writes and about two concrete
  names without reciprocal copying, where historical searches may be split.
  Only indices that can still receive writes are reported individually. Backing
  indices behind a data stream's write index never see a writer, so their
  aliases are not reported, and their unlinked fields are summarized once per
  data stream. `setup` does not rewrite historical documents.

## Upgrade and recovery

1. Install the corrected templates with `esdiag setup`.
2. Before mixing writer versions, roll over each affected data stream whose
   current write index has a provenance alias. For example, use
   `POST /metrics-diagnostic-esdiag/_rollover` for reports. This applies to both
   pre-rename indices with current-name aliases and indices created by the
   earlier branch templates with legacy-name aliases. Installing templates alone
   cannot change those existing field types. For standalone indices, write to a
   replacement index created with the corrected mappings.
3. Retry rejected diagnostics after rollover. Historical aliases remain useful
   for searches. If both names already exist as unlinked concrete fields,
   reindex into corrected mappings to make historical documents searchable under
   either name. Rollover fixes future writes only.

The copied values are indexed under both fields, increasing storage compared
with an alias. Retire the legacy fields and copying only after legacy writers
and dashboards are retired and old indices age out. The shipped saved-object
regression test checks dashboard references; writer retirement remains an
operational requirement.

See Elasticsearch's [copy_to reference](https://www.elastic.co/docs/reference/elasticsearch/mapping-reference/copy-to)
for indexing and source behavior. The opt-in test
`tests/provenance_writers_tests.rs` verifies both writer generations through real
Elasticsearch indexing, queries, aggregations, and rollover.

# User Diagnostic ID Fetcher

## Purpose

Identifies the current user and lists diagnostic IDs uploaded in the last 30 days so the user can choose a diagnostic bundle.

## Required Input

- `current_user`: the authenticated user identifier to match against `diagnostic.user`.

## Usage Guidance

- Use this reference only when the user's question matches the diagnostic area described above.
- Run the ES|QL examples that match the question and compare the returned values with the metric guidance below.
- Cite actual values from the ES|QL results; do not invent metrics, node names, index names, or thresholds.
- If the returned data is insufficient, say `not enough data` and identify the missing data.

## ES|QL Examples

### Recent Diagnostics For Current User

```esql
FROM "metrics-diagnostic-esdiag*"
| WHERE diagnostic.user == "{{current_user}}" AND @timestamp >= NOW() - 30 days
| KEEP diagnostic.id, diagnostic.user, @timestamp
| SORT @timestamp DESC
| LIMIT 10
```

## Metric Guidance

- `diagnostic.id`: Present these values as selectable diagnostic IDs. Use the newest row when the user asks for the latest diagnostic.
- `diagnostic.user`: Must match the authenticated user context; do not show diagnostics for a different user.
- `@timestamp`: Use this to order recent uploads and to enforce the 30-day lookup window.

## Response Guidance

- Interpret results using the metric guidance in this reference.
- Call out healthy patterns, investigation triggers, and recommended actions.
- Convert byte and millisecond values into human-readable units where useful.

## Dashboard

Relative dashboard URL example:

```text
/s/esdiag/app/dashboards#/view/user_diagnostic_id_fetcher?_g=(filters:!(('$state':(store:globalState),meta:(disabled:!f,index:a738e381-1538-4fb5-a7c1-eebbdff4623b,key:diagnostic.id,negate:!f,params:(query:'{{diagnostic_id}}'),type:phrase),query:(match_phrase:(diagnostic.id:'{{diagnostic_id}}')))),time:(from:now-1y%2Fd,to:now))
```

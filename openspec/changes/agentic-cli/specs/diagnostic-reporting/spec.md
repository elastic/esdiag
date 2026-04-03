## ADDED Requirements

### Requirement: Viewer-Aware Kibana Link Selection
The system SHALL determine the Kibana base URL for final processed-diagnostic reporting by first resolving the explicit output target's saved viewer host, and SHALL fall back to `ESDIAG_KIBANA_URL` when no saved viewer host is available. If `ESDIAG_KIBANA_SPACE` is present, the system SHALL append the configured space path to the selected Kibana base URL before constructing the final Kibana link.

#### Scenario: Saved viewer host overrides environment Kibana URL
- **GIVEN** a processed diagnostic is sent to a saved Elasticsearch host with role `send`
- **AND** that saved host references a saved Kibana viewer host
- **AND** `ESDIAG_KIBANA_URL` is also set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses the saved viewer host URL as its base URL

#### Scenario: Environment fallback is used when no saved viewer host exists
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is set
- **WHEN** final processing reporting builds the Kibana link
- **THEN** the link uses `ESDIAG_KIBANA_URL` as its base URL

#### Scenario: No Kibana link is reported when no source is available
- **GIVEN** a processed diagnostic completes without a resolved saved viewer host
- **AND** `ESDIAG_KIBANA_URL` is not set
- **WHEN** final processing reporting completes
- **THEN** no Kibana link is added to the final report output

# kibana-detection-engine Specification

## Purpose
Process Kibana Detection Engine health and rules from Kibana diagnostic bundles into ESDiag settings and metrics.

## Requirements

### Requirement: Kibana Detection Engine Processing
The system SHALL process Kibana Detection Engine health and rules.

#### Scenario: Successful detection engine rule processing
- **GIVEN** a Kibana diagnostic bundle that contains Detection Engine rule files
- **WHEN** the bundle is processed
- **THEN** the system exports them to the `settings-kibana.detection_engine-esdiag` data stream

#### Scenario: Successful detection engine health processing
- **GIVEN** a Kibana diagnostic bundle that contains Detection Engine health files
- **WHEN** the bundle is processed
- **THEN** the system exports them to the `metrics-kibana.detection_engine-esdiag` data stream

## ADDED Requirements

### Requirement: Web Asset Setup Requires Explicit Trial Consent
The web onboarding asset-setup stage SHALL present an unchecked "Start an
Elasticsearch trial license" choice, and SHALL pass it to the shared setup
pipeline as the trial opt-in. It MUST NOT start a trial license unless the
user checks that choice. When the license is inactive and the choice is
unchecked, the stage SHALL install Elasticsearch assets, skip Kibana assets,
and display the licensing warning instead of a Kibana error.

#### Scenario: Default asset setup on a basic license
- **GIVEN** the configured output's Agent Builder license is inactive
- **WHEN** the user runs the asset-setup stage without checking the trial choice
- **THEN** no `_license/start_trial` request is sent
- **AND** the stage reports that Kibana Agent Builder assets need an Enterprise or trial license

#### Scenario: User checks the trial choice
- **GIVEN** the configured output's Agent Builder license is inactive
- **WHEN** the user checks the trial choice and runs the asset-setup stage
- **THEN** a trial license is started before Kibana assets are installed

#### Scenario: Active license
- **GIVEN** the configured output's Agent Builder license is active
- **WHEN** the user runs the asset-setup stage
- **THEN** Elasticsearch and Kibana assets are installed regardless of the trial choice

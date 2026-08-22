## MODIFIED Requirements

### Requirement: Persistent Desktop Settings
This change SHALL leave web and desktop settings behavior unchanged. User mode
SHALL continue to read and write its active target and Kibana URL through
`settings.yml`. Service mode SHALL continue to avoid local credential and host
persistence. `esdiag init` and CLI output resolution MAY use `esdiag.yml`, but
they MUST NOT migrate, rewrite, or infer values from `settings.yml`.

#### Scenario: User mode retains legacy settings persistence
- **GIVEN** the web interface is running in `user` mode
- **WHEN** the user configures a custom target host and restarts the application without CLI arguments
- **THEN** the server initializes using `settings.yml` and its saved host target data
- **AND** no `esdiag.yml` migration occurs

#### Scenario: Service mode does not persist local credentials
- **GIVEN** the web interface is running in `service` mode
- **WHEN** a user updates available preferences from the UI
- **THEN** the system does not write credentials, application configuration, or host target records to local `esdiag.yml`, `settings.yml`, or `hosts.yml` artifacts

#### Scenario: CLI does not infer desktop preferences
- **GIVEN** user mode selects a saved output host in `settings.yml`
- **WHEN** a later CLI command omits its output target and no runtime output environment is present
- **THEN** the CLI resolves only `esdiag.yml` configuration
- **AND** it does not read `settings.yml`

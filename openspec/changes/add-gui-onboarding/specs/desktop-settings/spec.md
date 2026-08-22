## MODIFIED Requirements

### Requirement: Persistent Desktop Settings

The system SHALL support mode-aware settings persistence. In `user` mode, it
SHALL persist the active output deployment through `esdiag.yml` and its linked
saved hosts. Before replacing legacy `settings.yml` reads, it SHALL migrate
representable active-target and Kibana URL values into the shared application
configuration, retain a backup, and report any value that cannot be migrated.
In `service` mode, it SHALL avoid local credential and host persistence and
only retain limited, non-sensitive preferences.

#### Scenario: User mode migrates legacy output settings

- **GIVEN** user mode has legacy settings with a representable saved output target
- **WHEN** the upgraded web interface starts
- **THEN** it creates an equivalent shared output configuration
- **AND** retains a backup of the legacy settings before switching persistence

#### Scenario: User mode persists shared output settings

- **GIVEN** the web interface is running in `user` mode after migration
- **WHEN** the user configures a custom saved output target and restarts the application
- **THEN** the server initializes from `esdiag.yml` and linked host target data

#### Scenario: Service mode does not persist local credentials

- **GIVEN** the web interface is running in `service` mode
- **WHEN** a user updates available preferences from the UI
- **THEN** the system does not write credentials or host target records to local `settings.yml`, `esdiag.yml`, or `hosts.yml` artifacts

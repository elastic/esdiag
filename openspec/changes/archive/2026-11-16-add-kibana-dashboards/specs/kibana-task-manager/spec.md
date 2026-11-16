## ADDED Requirements

### Requirement: Task Manager Task Type Documents
The system SHALL export one document per Task Manager task type to `metrics-kibana.task_manager.task_type-esdiag`. Each document SHALL merge the task type entries from the workload, runtime drift, execution duration, and execution result frequency stats of `kibana_task_manager_health.json`. Each document SHALL carry the shared Kibana metadata.

#### Scenario: Task type merged across sections
- **GIVEN** a Task Manager health response in which task type `alerting:.index-threshold` appears in `stats.workload.value.task_types`, `stats.runtime.value.drift_by_type`, `stats.runtime.value.execution.duration`, and `stats.runtime.value.execution.result_frequency_percent_as_number`
- **WHEN** the response is processed
- **THEN** exactly one document with `task_type.name` equal to `alerting:.index-threshold` is exported to `metrics-kibana.task_manager.task_type-esdiag`
- **AND** the document contains `task_type.workload`, `task_type.drift`, `task_type.duration`, and `task_type.result_frequency` from those sections
- **AND** the document contains the same `diagnostic`, `node`, and `@timestamp` values as the summary document

#### Scenario: Task type present in only some sections
- **GIVEN** a task type that appears in the workload stats but not in the runtime stats
- **WHEN** the response is processed
- **THEN** its document contains `task_type.workload`, and the absent sections are omitted

#### Scenario: No task types
- **GIVEN** a Task Manager health response that contains no per-task-type entries
- **WHEN** the response is processed
- **THEN** no documents are exported to `metrics-kibana.task_manager.task_type-esdiag`
- **AND** the summary document is still exported

### Requirement: Task Manager Summary Document Shape
The summary document in `metrics-kibana.task_manager-esdiag` SHALL exclude the per-task-type maps `stats.workload.value.task_types`, `stats.runtime.value.drift_by_type`, `stats.runtime.value.execution.duration`, and `stats.runtime.value.execution.result_frequency_percent_as_number`. It SHALL keep every other field of the health response.

#### Scenario: Per-task-type maps removed
- **GIVEN** a Task Manager health response that contains per-task-type maps
- **WHEN** the summary document is exported
- **THEN** the summary document does not contain those maps
- **AND** it contains `status`, `stats.configuration`, `stats.runtime.value.drift`, `stats.runtime.value.load`, `stats.runtime.value.polling`, `stats.workload.value.count`, and `stats.capacity_estimation` when they are present in the response

### Requirement: Task Manager Dashboard Field Mappings
The index templates for `metrics-kibana.task_manager-esdiag` and `metrics-kibana.task_manager.task_type-esdiag` SHALL explicitly map every field that the Kibana Task Manager dashboard uses. Status and name fields SHALL be `keyword`, counts SHALL be `long`, percentiles, percentages, and throughput SHALL be `double`, and poll times SHALL be `date`.

#### Scenario: Fractional value after integer value
- **GIVEN** a template has been installed and a first document holds an integer p99 drift
- **WHEN** a later document holds a fractional p99 drift
- **THEN** the fractional value is indexed without truncation

#### Scenario: Missing section still resolvable
- **GIVEN** no indexed document contains `stats.capacity_estimation`
- **WHEN** the Task Manager data view loads its fields
- **THEN** the capacity estimation fields used by the dashboard are present in the field list

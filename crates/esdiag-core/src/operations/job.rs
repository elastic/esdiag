// Copyright Elasticsearch B.V. and/or licensed to Elasticsearch B.V. under one
// or more contributor license agreements. Licensed under the Elastic License 2.0;
// you may not use this file except in compliance with the Elastic License 2.0.

use super::{job_outcome, saved_job_result};
use crate::{
    cli_output::{CliOutcome, CommandOutcome},
    data::{ApplicationConfig, Job, JobOutput, load_saved_jobs},
    job::model::ExportTarget,
    processor::Identifiers,
};
use eyre::{Result, eyre};

pub fn job_list() -> Result<CommandOutcome> {
    let jobs = load_saved_jobs()?
        .into_iter()
        .map(|(name, job)| saved_job_result(name, &job))
        .collect();
    Ok(CommandOutcome::Completed(CliOutcome::JobsListed { jobs }))
}

/// Whether the front end can give stdout to processed documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdoutPolicy {
    Allow,
    Reject,
}

/// [`StdoutPolicy::Reject`] refused a saved job that exports processed
/// documents to stdout.
#[derive(Debug)]
pub struct SavedJobStdoutRejected {
    pub name: String,
}

impl std::fmt::Display for SavedJobStdoutRejected {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Saved job '{}' exports processed documents to stdout, which is not available here",
            self.name
        )
    }
}

impl std::error::Error for SavedJobStdoutRejected {}

/// Runs a saved job. Returns [`CommandOutcome::Streamed`] when the job exports
/// processed documents to stdout. The policy check and the run use one loaded
/// copy of the job.
pub async fn job_run(name: String, stdout: StdoutPolicy) -> Result<CommandOutcome> {
    let job = crate::job::load_saved_job(&name)?;
    run_loaded(name, stdout, job).await
}

/// Runs a saved job on behalf of `user`, as web jobs do: the report records
/// `user`, and `account` unless the saved job names its own.
pub async fn job_run_as(
    name: String,
    stdout: StdoutPolicy,
    user: String,
    account: Option<String>,
) -> Result<CommandOutcome> {
    let mut job = crate::job::load_saved_job(&name)?;
    requested_by(&mut job.identifiers, user, account);
    run_loaded(name, stdout, job).await
}

fn requested_by(identifiers: &mut Identifiers, user: String, account: Option<String>) {
    identifiers.user = Some(user);
    if identifiers.account.is_none() {
        identifiers.account = account;
    }
}

async fn run_loaded(name: String, stdout: StdoutPolicy, job: Job) -> Result<CommandOutcome> {
    let writes_stdout = job_writes_stdout(&job);
    if writes_stdout && stdout == StdoutPolicy::Reject {
        return Err(SavedJobStdoutRejected { name }.into());
    }
    let outcome = crate::job::run_saved_job(&name, job).await?;
    if writes_stdout {
        Ok(CommandOutcome::Streamed)
    } else {
        Ok(CommandOutcome::Completed(job_outcome(name, outcome)))
    }
}

pub fn job_delete(name: String) -> Result<CommandOutcome> {
    crate::job::handle_job_delete(&name)?;
    Ok(CommandOutcome::Completed(CliOutcome::JobDeleted { name }))
}

pub fn job_save(name: &str, job: Job) -> Result<()> {
    crate::job::save_job(name, job)
}

/// Whether the named saved job exports processed documents to stdout.
/// Unknown names return `false`; [`job_run`] reports them. Only for choosing
/// log routing before a run; [`job_run`] decides from the job it executes.
pub fn saved_job_writes_stdout(name: &str) -> Result<bool> {
    Ok(load_saved_jobs()?.get(name).is_some_and(job_writes_stdout))
}

fn job_writes_stdout(job: &Job) -> bool {
    job.process()
        .is_some_and(|process| matches!(process.export, ExportTarget::Stdout))
}

pub fn derive_collect_job(
    host: &str,
    output: &str,
    diagnostic_type: &str,
    send_id: Option<&str>,
    identifiers: Identifiers,
) -> Result<Job> {
    let builder = Job::builder()
        .identifiers(identifiers)
        .collect_from(host)?
        .diagnostic_type(diagnostic_type);

    match send_id {
        Some(send_id) => builder.send_to(send_id),
        None => builder.collect_to(output),
    }
}

pub fn derive_process_job(input: &str, output: Option<&str>, identifiers: Identifiers) -> Result<Job> {
    let output = match output {
        Some(output) => JobOutput::from_cli_target(output)?,
        None => {
            let config = ApplicationConfig::load()?;
            if let Some(context) = config.output.elastic_context {
                JobOutput::KnownHost {
                    name: context.host_reference()?,
                }
            } else if let Some(name) = config.output.default {
                JobOutput::KnownHost { name }
            } else {
                return Err(eyre!(
                    "Saved jobs require an explicit or configured process output target"
                ));
            }
        }
    };
    Job::builder()
        .identifiers(identifiers)
        .collect_from(input)?
        .process_to(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        TestEnv,
        data::{Application, HostRole, KnownHost},
    };
    use url::Url;

    fn setup_env() -> TestEnv {
        let mut env = TestEnv::new();
        env.set("ESDIAG_KEYSTORE_PASSWORD", "pw");
        env
    }

    fn save_collect_host(name: &str) {
        KnownHost::new_no_auth(
            Application::Elasticsearch,
            Url::parse("http://localhost:9200").expect("valid url"),
            vec![HostRole::Collect],
            None,
            false,
        )
        .save(name)
        .expect("save known host");
    }

    #[test]
    fn derive_collect_job_requires_known_host_input() {
        let _env = setup_env();
        let err = match derive_collect_job(
            "https://example.com",
            "diag-dir",
            "standard",
            None,
            Identifiers::default(),
        ) {
            Ok(_) => panic!("non-host input should be rejected"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("Jobs require a saved known host name or Elastic CLI context as input")
        );
    }

    #[test]
    fn derive_collect_job_uses_output_dir_without_save_dir() {
        let _env = setup_env();
        save_collect_host("prod-es");

        let job = derive_collect_job("prod-es", "/tmp/esdiag-output", "support", None, Identifiers::default())
            .expect("derive collect job");

        assert!(job.process().is_none());
        assert!(job.send().is_none());
        assert_eq!(
            job.save().and_then(|save| save.dir.as_ref()),
            Some(&std::path::PathBuf::from("/tmp/esdiag-output"))
        );
    }

    #[test]
    fn derive_process_job_requires_explicit_or_configured_output() {
        let _env = setup_env();
        save_collect_host("prod-es");
        let err = match derive_process_job("prod-es", None, Identifiers::default()) {
            Ok(_) => panic!("missing process output should be rejected"),
            Err(err) => err,
        };
        assert_eq!(
            err.to_string(),
            "Saved jobs require an explicit or configured process output target"
        );
    }

    #[test]
    fn saved_job_writes_stdout_only_for_stdout_exports() {
        let _env = setup_env();
        save_collect_host("prod-es");
        let stdout_job = Job::try_new(
            Identifiers::default(),
            crate::job::model::Input::Collect {
                host: "prod-es".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(crate::job::model::Process {
                selection: None,
                export: ExportTarget::Stdout,
            }),
            None,
        )
        .expect("valid streaming job");
        job_save("stdout-job", stdout_job).expect("save stdout job");
        let collect_job = derive_collect_job(
            "prod-es",
            "/tmp/esdiag-output",
            "standard",
            None,
            Identifiers::default(),
        )
        .expect("collect job");
        job_save("collect-job", collect_job).expect("save collect job");

        assert!(saved_job_writes_stdout("stdout-job").expect("stdout job"));
        assert!(!saved_job_writes_stdout("collect-job").expect("collect job"));
        assert!(!saved_job_writes_stdout("missing").expect("missing job"));
    }

    #[test]
    fn a_requested_job_records_the_requester_and_keeps_its_own_account() {
        let mut saved = Identifiers {
            user: Some("saver".to_string()),
            account: Some("saved-account".to_string()),
            ..Default::default()
        };
        requested_by(&mut saved, "alice".to_string(), Some("acme".to_string()));
        assert_eq!(saved.user.as_deref(), Some("alice"));
        assert_eq!(saved.account.as_deref(), Some("saved-account"));

        let mut unnamed = Identifiers::default();
        requested_by(&mut unnamed, "alice".to_string(), Some("acme".to_string()));
        assert_eq!(unnamed.account.as_deref(), Some("acme"));
    }

    #[tokio::test]
    async fn job_run_rejects_stdout_export_before_running() {
        let _env = setup_env();
        save_collect_host("prod-es");
        let stdout_job = Job::try_new(
            Identifiers::default(),
            crate::job::model::Input::Collect {
                host: "prod-es".to_string(),
                diagnostic_type: "standard".to_string(),
                include: None,
                exclude: None,
            },
            None,
            Some(crate::job::model::Process {
                selection: None,
                export: ExportTarget::Stdout,
            }),
            None,
        )
        .expect("valid streaming job");
        job_save("stdout-job", stdout_job).expect("save stdout job");

        let err = job_run("stdout-job".to_string(), StdoutPolicy::Reject)
            .await
            .expect_err("stdout export should be rejected");
        assert!(
            err.to_string().contains("exports processed documents to stdout"),
            "{err}"
        );
    }
}

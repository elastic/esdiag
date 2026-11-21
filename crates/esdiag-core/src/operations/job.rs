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

/// Runs a saved job. Returns [`CommandOutcome::Streamed`] when the job exports
/// processed documents to stdout; check [`saved_job_writes_stdout`] first when
/// stdout is not available for documents.
pub async fn job_run(name: String) -> Result<CommandOutcome> {
    let stdout_owned = saved_job_writes_stdout(&name)?;
    let outcome = crate::job::handle_job_run(&name).await?;
    if stdout_owned {
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
/// Unknown names return `false`; [`job_run`] reports them.
pub fn saved_job_writes_stdout(name: &str) -> Result<bool> {
    Ok(load_saved_jobs()?
        .get(name)
        .and_then(|job| job.process())
        .is_some_and(|process| matches!(process.export, ExportTarget::Stdout)))
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
}

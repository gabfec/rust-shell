use std::io::{self, Write};
use std::process::Child;

/// A background pipeline: one or more processes started from one command line.
struct Job {
    id: usize,
    command: String,
    children: Vec<Child>,
}

impl Job {
    fn is_done(&mut self) -> bool {
        self.children
            .iter_mut()
            .all(|child| matches!(child.try_wait(), Ok(Some(_))))
    }
}

#[derive(Default)]
pub struct JobTable {
    jobs: Vec<Job>,
}

/// A snapshot of one job, as shown by `jobs` and the pre-prompt report.
pub struct JobStatus {
    id: usize,
    marker: char,
    command: String,
    done: bool,
}

impl JobTable {
    /// Registers a background job under the smallest free job number.
    pub fn add(&mut self, command: String, children: Vec<Child>) -> usize {
        let id = (1..)
            .find(|id| !self.jobs.iter().any(|job| job.id == *id))
            .expect("job ids are unbounded");
        self.jobs.push(Job {
            id,
            command,
            children,
        });
        id
    }

    /// Polls every job, drops the finished ones, and returns the status of
    /// all jobs (finished included) with their `+`/`-` markers.
    pub fn reap(&mut self) -> Vec<JobStatus> {
        let n = self.jobs.len();
        let statuses = self
            .jobs
            .iter_mut()
            .enumerate()
            .map(|(i, job)| JobStatus {
                id: job.id,
                marker: match n - i {
                    1 => '+',
                    2 => '-',
                    _ => ' ',
                },
                command: job.command.clone(),
                done: job.is_done(),
            })
            .collect::<Vec<_>>();
        self.jobs.retain(|job| {
            !statuses
                .iter()
                .any(|status| status.id == job.id && status.done)
        });
        statuses
    }
}

/// Prints finished jobs, and running ones too if `show_running` is set.
pub fn print_jobs(
    statuses: &[JobStatus],
    show_running: bool,
    out: &mut dyn Write,
) -> io::Result<()> {
    for s in statuses {
        if s.done {
            writeln!(out, "[{}]{}  {:<24}{}", s.id, s.marker, "Done", s.command)?;
        } else if show_running {
            writeln!(
                out,
                "[{}]{}  {:<24}{} &",
                s.id, s.marker, "Running", s.command
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    /// A process that runs until the job table (and so its stdin) is dropped.
    fn running() -> Child {
        Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn finished() -> Child {
        let mut child = Command::new("true").spawn().unwrap();
        child.wait().unwrap();
        child
    }

    fn printed(statuses: &[JobStatus], show_running: bool) -> String {
        let mut out = Vec::new();
        print_jobs(statuses, show_running, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn ids_count_up_from_one() {
        let mut jobs = JobTable::default();
        assert_eq!(jobs.add("a".into(), vec![running()]), 1);
        assert_eq!(jobs.add("b".into(), vec![running()]), 2);
        assert_eq!(jobs.add("c".into(), vec![running()]), 3);
    }

    #[test]
    fn smallest_free_id_is_reused() {
        let mut jobs = JobTable::default();
        jobs.add("a".into(), vec![running()]);
        jobs.add("b".into(), vec![finished()]);
        jobs.add("c".into(), vec![running()]);
        jobs.reap(); // removes job 2
        assert_eq!(jobs.add("d".into(), vec![running()]), 2);
        assert_eq!(jobs.add("e".into(), vec![running()]), 4);
    }

    #[test]
    fn markers_flag_the_two_most_recent_jobs() {
        let mut jobs = JobTable::default();
        for name in ["a", "b", "c"] {
            jobs.add(name.into(), vec![running()]);
        }
        let expected = format!(
            "[1]   {r:<24}a &\n[2]-  {r:<24}b &\n[3]+  {r:<24}c &\n",
            r = "Running"
        );
        assert_eq!(printed(&jobs.reap(), true), expected);
    }

    #[test]
    fn finished_jobs_are_reported_once_then_removed() {
        let mut jobs = JobTable::default();
        jobs.add("sleeper".into(), vec![running()]);
        jobs.add("quick".into(), vec![finished()]);

        let statuses = jobs.reap();
        assert_eq!(
            printed(&statuses, false),
            format!("[2]+  {:<24}quick\n", "Done")
        );
        assert_eq!(
            printed(&jobs.reap(), true),
            format!("[1]+  {:<24}sleeper &\n", "Running")
        );
    }

    #[test]
    fn a_pipeline_job_is_done_only_when_every_process_is() {
        let mut jobs = JobTable::default();
        jobs.add("a | b".into(), vec![finished(), running()]);
        assert!(printed(&jobs.reap(), false).is_empty());

        jobs.add("c | d".into(), vec![finished(), finished()]);
        assert_eq!(
            printed(&jobs.reap(), false),
            format!("[2]+  {:<24}c | d\n", "Done")
        );
    }
}

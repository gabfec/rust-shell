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

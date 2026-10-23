use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

use console::style;
use indicatif::{HumanBytes, HumanCount, ProgressBar, ProgressStyle};

use crate::interrupt;
use crate::stats::Statistics;
use crate::volume::Volume;

const TICK: Duration = Duration::from_millis(100);

pub fn usage() -> String {
    format!(
        "\
rmp {version}
Delete files and directories in parallel, with a progress bar.

Usage: rmp [OPTIONS] PATH...

Directories are deleted recursively. Symlinks are deleted, never followed.
rmp counts everything first and then deletes it, reporting each failure as a
warning and carrying on with the rest. Ctrl-C stops it cleanly.

Options:
  -h, --help     Print this help
  -V, --version  Print the version
  --             Treat every argument after this one as a path
",
        version = env!("CARGO_PKG_VERSION")
    )
}

pub fn warn(message: impl Display) {
    eprintln!("{} {message}", style("warning:").yellow().bold().for_stderr());
}

pub fn error(message: impl Display) {
    eprintln!("{} {message}", style("error:").red().bold().for_stderr());
}

/// Reports that `signal` stopped the run.
pub fn stopped(signal: i32, detail: &str) {
    eprintln!(
        "{} by {}{detail}",
        style("Stopped").yellow().bold().for_stderr(),
        interrupt::name(signal)
    );
}

/// The live counter line shown while counting.
pub struct CountLine(ProgressBar);

impl CountLine {
    pub fn new() -> Self {
        let style = ProgressStyle::with_template("{spinner:.cyan} Counting {msg}")
            .expect("the counter template is valid");
        let spinner = ProgressBar::new_spinner().with_style(style);
        spinner.enable_steady_tick(TICK);
        Self(spinner)
    }

    pub fn update(&self, stats: &Statistics) {
        self.0.set_message(summary(stats));
    }

    /// Replaces the counter with a line holding the totals and the free space on each of
    /// `volumes` before and after deleting.
    pub fn finish(self, stats: &Statistics, volumes: &[Volume]) {
        self.0.finish_and_clear();
        let free = free_space(
            volumes.iter().map(|volume| {
                let change =
                    format!("{} -> ~{}", HumanBytes(volume.before), HumanBytes(volume.expected));
                (volume.target.path.as_path(), change)
            }),
            volumes.len() > 1,
        );
        eprintln!("{} {}{free}", style("Counted").cyan().bold().for_stderr(), summary(stats));
    }
}

/// Files and directories in `stats` with thousands separators, and their size.
fn summary(stats: &Statistics) -> String {
    format!(
        "{}, {}, {}",
        tally(stats.files, "file", "files"),
        tally(stats.dirs, "directory", "directories"),
        HumanBytes(stats.bytes)
    )
}

/// ` (Free space ...)` holding each value, followed by the target it was read `on` when there are
/// `several` volumes, or nothing when there are no values.
fn free_space<'a>(values: impl Iterator<Item = (&'a Path, String)>, several: bool) -> String {
    let values: Vec<_> = values
        .map(|(on, value)| match several {
            false => value,
            true => format!("{value} on '{}'", on.display()),
        })
        .collect();
    if values.is_empty() { String::new() } else { format!(" (Free space {})", values.join(", ")) }
}

/// `count` with thousands separators, followed by `one` or `many`, whichever agrees with it.
fn tally(count: u64, one: &str, many: &str) -> String {
    format!("{} {}", HumanCount(count), if count == 1 { one } else { many })
}

/// The progress bar shown while deleting, measured in bytes.
pub struct DeleteBar {
    bar: ProgressBar,
    total: Statistics,
}

impl DeleteBar {
    pub fn new(total: Statistics) -> Self {
        let style = ProgressStyle::with_template(
            "{wide_bar:.red/white} {binary_bytes}/{binary_total_bytes} ({eta} left) {msg}",
        )
        .expect("the progress bar template is valid");
        let bar = ProgressBar::new(total.bytes).with_style(style);
        bar.enable_steady_tick(TICK);
        Self { bar, total }
    }

    pub fn update(&self, done: &Statistics) {
        self.bar.set_position(done.bytes);
        self.bar.set_message(format!(
            "{}/{} entries",
            HumanCount(done.entries()),
            HumanCount(self.total.entries())
        ));
    }

    /// Prints a warning above the bar.
    pub fn warn(&self, message: impl Display) {
        self.bar.suspend(|| warn(message));
    }

    /// Replaces the bar with a line holding the time taken and the free space now on each volume
    /// in `after` whose free space differs from the expected, then warns about how many files and
    /// directories in `done` differ from the ones counted.
    pub fn finish(self, done: &Statistics, after: &[(&Volume, u64)], failures: u64) {
        self.bar.finish_and_clear();
        let free = free_space(
            after.iter().filter_map(|&(volume, free)| {
                let shown = HumanBytes(free).to_string();
                let differs = shown != HumanBytes(volume.expected).to_string();
                differs.then_some((volume.target.path.as_path(), shown))
            }),
            after.len() > 1,
        );
        eprintln!(
            "{} in {:.3} seconds{free}",
            style("Deleted").green().bold().for_stderr(),
            self.bar.elapsed().as_secs_f64()
        );
        let mut left = Vec::new();
        let mut extra = Vec::new();
        for (deleted, counted, one, many) in [
            (done.files, self.total.files, "file", "files"),
            (done.dirs, self.total.dirs, "directory", "directories"),
        ] {
            if deleted < counted {
                left.push(tally(counted - deleted, one, many));
            }
            if deleted > counted {
                extra.push(tally(deleted - counted, one, many));
            }
        }
        if !left.is_empty() {
            warn(format!("{} not deleted", left.join(" and ")));
        }
        if !extra.is_empty() {
            warn(format!("deleted {} more than counted", extra.join(" and ")));
        }
        if failures > 0 {
            warn(format!("{}, reported above", tally(failures, "failure", "failures")));
        }
    }
}

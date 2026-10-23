use std::fmt::Display;
use std::path::Path;
use std::time::Duration;

use console::style;
use indicatif::{HumanBytes, HumanCount, HumanDuration, ProgressBar, ProgressStyle};

use crate::interrupt;
use crate::stats::Statistics;

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

/// Reports `free` bytes of free space and what it becomes once `freed` more bytes are freed,
/// followed by the target `on` the filesystem when one is given.
pub fn free_space(free: u64, freed: u64, on: Option<&Path>) {
    let on =
        on.map_or(String::new(), |path| format!(" on the volume holding '{}'", path.display()));
    eprintln!("Free space: {} -> {}{on}", HumanBytes(free), HumanBytes(free + freed));
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
        self.0.set_message(exact(stats));
    }

    /// Replaces the counter with a line holding the totals.
    pub fn finish(self, stats: &Statistics) {
        self.0.finish_and_clear();
        eprintln!("{} {}", style("Counted").cyan().bold().for_stderr(), exact(stats));
    }
}

/// Files, directories, and bytes in `stats`, as exact numbers with thousands separators.
fn exact(stats: &Statistics) -> String {
    format!(
        "{}, {}, {}",
        tally(stats.files, "file", "files"),
        tally(stats.dirs, "directory", "directories"),
        tally(stats.bytes, "byte", "bytes")
    )
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

    /// Replaces the bar with a line holding the human-readable amount deleted and the time taken,
    /// then warns about any of `done`'s tallies that differ from the ones counted.
    pub fn finish(self, done: &Statistics, failures: u64) {
        self.bar.finish_and_clear();
        eprintln!(
            "{} {} in {}",
            style("Deleted").green().bold().for_stderr(),
            HumanBytes(done.bytes),
            HumanDuration(self.bar.elapsed())
        );
        let (deleted, counted): (Vec<_>, Vec<_>) = [
            (done.files, self.total.files, "file", "files"),
            (done.dirs, self.total.dirs, "directory", "directories"),
            (done.bytes, self.total.bytes, "byte", "bytes"),
        ]
        .into_iter()
        .filter(|(deleted, counted, ..)| deleted != counted)
        .map(|(deleted, counted, one, many)| (tally(deleted, one, many), tally(counted, one, many)))
        .unzip();
        if !deleted.is_empty() {
            warn(format!("deleted {}, but counted {}", deleted.join(", "), counted.join(", ")));
        }
        if failures > 0 {
            warn(format!("{}, reported above", tally(failures, "failure", "failures")));
        }
    }
}

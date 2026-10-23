use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

use tempfile::TempDir;

fn rmp(args: &[&OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rmp")).args(args).output().unwrap()
}

#[test]
fn deletes_reports_warnings_and_prints_usage_without_color() {
    let usage = rmp(&[]);
    assert_eq!(usage.status.code(), Some(2), "no operands is a usage error");
    assert!(String::from_utf8_lossy(&usage.stderr).contains("Usage: rmp"));

    let tmp = TempDir::new().unwrap();
    let doomed = tmp.path().join("doomed");
    fs::create_dir_all(doomed.join("sub")).unwrap();
    fs::write(doomed.join("sub/file"), vec![0; 2048]).unwrap();
    let locked = tmp.path().join("held/locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("stuck"), "").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let missing = tmp.path().join("missing");

    let run = rmp(&[doomed.as_os_str(), tmp.path().join("held").as_os_str(), missing.as_os_str()]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "a missing operand fails the run: {stderr}");
    assert!(!doomed.exists(), "the existing operand is still deleted");
    assert!(stderr.contains("warning: cannot remove"), "{stderr}");
    let counted = "Counted 2 files, 4 directories, 2.00 KiB (Free space ";
    assert!(stderr.contains(counted), "the count ends with the free space: {stderr}");
    assert!(stderr.contains(" -> ~"), "the free space afterward is an estimate: {stderr}");
    let deleted = stderr.lines().find_map(|line| line.strip_prefix("Deleted in ")).unwrap();
    let seconds = deleted.split_once(" seconds").unwrap().0;
    assert_eq!(seconds.split_once('.').unwrap().1.len(), 3, "milliseconds: {stderr}");
    assert!(
        stderr.contains("warning: 1 file and 2 directories not deleted\n"),
        "only the files and directories left behind are reported: {stderr}"
    );
    assert!(!stderr.contains('\x1b'), "no color or progress bar when stderr is not a terminal");
}

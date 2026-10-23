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
    let counted = "Counted 2 files, 4 directories, 2,048 bytes\nFree space: ";
    assert!(stderr.contains(counted), "free space follows the count: {stderr}");
    assert!(stderr.contains("Deleted 2.00 KiB in"), "{stderr}");
    assert!(
        stderr.contains(
            "warning: deleted 1 file, 2 directories, but counted 2 files, 4 directories\n"
        ),
        "only the tallies that differ from the count are reported: {stderr}"
    );
    assert!(!stderr.contains('\x1b'), "no color or progress bar when stderr is not a terminal");
}

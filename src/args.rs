use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    /// Delete `paths`, ignoring the `rm` options in `ignored`.
    Remove {
        paths: Vec<PathBuf>,
        ignored: Vec<OsString>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgsError {
    NoOperands,
    UnknownOption(OsString),
}

/// Parses the arguments that follow the program name. Everything after `--` is a path, and `-r`,
/// `-f`, and `-rf` are accepted and ignored.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, ArgsError> {
    let mut operands = Vec::new();
    let mut ignored = Vec::new();
    let mut options_ended = false;
    for arg in args {
        if options_ended {
            operands.push(arg.into());
            continue;
        }
        match arg.as_encoded_bytes() {
            b"--" => options_ended = true,
            b"-h" | b"--help" => return Ok(Command::Help),
            b"-V" | b"--version" => return Ok(Command::Version),
            b"-r" | b"-f" | b"-rf" => ignored.push(arg),
            [b'-', _, ..] => return Err(ArgsError::UnknownOption(arg)),
            _ => operands.push(arg.into()),
        }
    }
    if operands.is_empty() {
        Err(ArgsError::NoOperands)
    } else {
        Ok(Command::Remove { paths: operands, ignored })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, ArgsError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_operands_options_and_separator() {
        assert_eq!(parse_strs(&[]), Err(ArgsError::NoOperands));
        assert_eq!(parse_strs(&["--"]), Err(ArgsError::NoOperands));
        assert_eq!(parse_strs(&["a", "-h"]), Ok(Command::Help), "help wins over operands");
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
        assert_eq!(
            parse_strs(&["-rf", "a", "-r", "-f"]),
            Ok(Command::Remove {
                paths: vec!["a".into()],
                ignored: vec!["-rf".into(), "-r".into(), "-f".into()]
            }),
            "rm's -r, -f, and -rf are ignored"
        );
        assert_eq!(parse_strs(&["-fr", "a"]), Err(ArgsError::UnknownOption("-fr".into())));
        assert_eq!(
            parse_strs(&["a", "-", "--", "-h", "--", "-rf"]),
            Ok(Command::Remove {
                paths: vec!["a".into(), "-".into(), "-h".into(), "--".into(), "-rf".into()],
                ignored: Vec::new()
            }),
            "a lone dash is a path, and everything after the first -- is a path"
        );
    }
}

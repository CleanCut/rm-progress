use std::ffi::OsString;
use std::path::PathBuf;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    Version,
    Remove(Vec<PathBuf>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ArgsError {
    NoOperands,
    UnknownOption(OsString),
}

/// Parses the arguments that follow the program name. Everything after `--` is a path.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, ArgsError> {
    let mut operands = Vec::new();
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
            [b'-', _, ..] => return Err(ArgsError::UnknownOption(arg)),
            _ => operands.push(arg.into()),
        }
    }
    if operands.is_empty() { Err(ArgsError::NoOperands) } else { Ok(Command::Remove(operands)) }
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
        assert_eq!(parse_strs(&["-rf", "a"]), Err(ArgsError::UnknownOption("-rf".into())));
        assert_eq!(
            parse_strs(&["a", "-", "--", "-h", "--"]),
            Ok(Command::Remove(vec!["a".into(), "-".into(), "-h".into(), "--".into()])),
            "a lone dash is a path, and everything after the first -- is a path"
        );
    }
}

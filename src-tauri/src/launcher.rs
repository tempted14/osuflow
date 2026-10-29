//! Native argument-safe launcher shared by stable/Wine and lazer. Never expands a shell.
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub prefix: Option<PathBuf>,
}

/// A custom command is parsed into argv; {file} is substituted as one argument, never shell code.
pub fn custom_launch(command: &str, archive: &Path, prefix: Option<PathBuf>) -> io::Result<Launch> {
    let mut words = shlex::split(command)
        .filter(|words| !words.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Launch command has unmatched quotes or is empty",
            )
        })?;
    let program = words.remove(0);
    if program.contains("{file}") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "{file} belongs in a launch argument, not the program name",
        ));
    }
    let mut has_file = false;
    let mut args = words
        .into_iter()
        .map(|word| {
            if word.contains("{file}") {
                has_file = true;
            }
            if word == "{file}" {
                archive.as_os_str().to_owned()
            } else {
                OsString::from(word.replace("{file}", &archive.to_string_lossy()))
            }
        })
        .collect::<Vec<_>>();
    if !has_file {
        args.push(archive.as_os_str().to_owned());
    }
    Ok(Launch {
        program: program.into(),
        args,
        prefix,
    })
}

/// A helper that exits unsuccessfully is a failed request. A long-lived game is left running.
/// Returning Ok confirms the launch request only; callers decide how to verify the import.
pub fn run(launch: Launch, directory: &Path) -> io::Result<()> {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut command = Command::new(launch.program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for helper consoles.
    }
    command
        .args(launch.args)
        .current_dir(directory)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(prefix) = launch.prefix {
        command.env("WINEPREFIX", prefix);
    }
    let mut child = command.spawn()?;
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                return Err(io::Error::other(format!(
                    "The import helper exited with {status}"
                )));
            }
            None if started.elapsed() >= Duration::from_secs(2) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                return Ok(());
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

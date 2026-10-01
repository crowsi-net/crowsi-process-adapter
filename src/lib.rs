//! Crowsi physical process mechanism, 2026. No Work, identity, RPC or retry policy.
//! Pipes belong to Crowsi framing callers. No unbounded output capture is created.
use processkit::{Outcome, OutputBufferPolicy, ProcessGroup, ProcessGroupOptions};
use std::{ffi::OsString, io, path::PathBuf, time::Duration};
use tokio::io::{AsyncReadExt, DuplexStream};
mod pipe;

#[derive(Debug)]
pub enum Error {
    InvalidLaunch,
    Mechanism(processkit::Error),
    Io(io::Error),
    ReapTimeout,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never publish child arguments/environment or untrusted mechanism text.
        f.write_str(match self {
            Self::InvalidLaunch => "InvalidLaunch",
            Self::Mechanism(_) => "ProcessMechanismFailed",
            Self::Io(_) => "ProcessIoFailed",
            Self::ReapTimeout => "ReapTimeout",
        })
    }
}
impl std::error::Error for Error {}
impl From<processkit::Error> for Error {
    fn from(v: processkit::Error) -> Self {
        Self::Mechanism(v)
    }
}
impl From<io::Error> for Error {
    fn from(v: io::Error) -> Self {
        Self::Io(v)
    }
}

/// Caller-owned launch configuration. Executable selection/verification is owner policy.
#[derive(Debug)]
pub struct Launch {
    executable: PathBuf,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    directory: Option<PathBuf>,
}
impl Launch {
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            env: Vec::new(),
            directory: None,
        }
    }
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
    pub fn directory(mut self, directory: impl Into<PathBuf>) -> Self {
        self.directory = Some(directory.into());
        self
    }
    fn validate(&self) -> Result<(), Error> {
        let size = self.executable.as_os_str().len()
            + self.args.iter().map(|a| a.len()).sum::<usize>()
            + self
                .env
                .iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>();
        if !self.executable.is_absolute()
            || self.directory.as_ref().is_some_and(|p| !p.is_absolute())
            || self.args.len() > 128
            || self.env.len() > 64
            || size > 65536
        {
            return Err(Error::InvalidLaunch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Capabilities {
    pub containment: &'static str,
    pub parent_death_scope: &'static str,
}

/// One owned physical tree. Never attaches to a persisted PID or restarts a request.
#[derive(Debug)]
pub struct OwnedProcess {
    group: ProcessGroup,
    waiter: tokio::task::JoinHandle<processkit::Result<Outcome>>,
    input_task: tokio::task::JoinHandle<io::Result<()>>,
    pid: Option<u32>,
    stdin: Option<DuplexStream>,
    stdout: Option<DuplexStream>,
    stderr: Option<DuplexStream>,
    exit: Option<Outcome>,
    waiter_consumed: bool,
    stop_output: processkit::CancellationToken,
}
impl OwnedProcess {
    #[cfg(target_os = "linux")]
    pub async fn spawn(launch: Launch) -> Result<Self, Error> {
        launch.validate()?;
        let group = ProcessGroup::with_options(
            ProcessGroupOptions::default().shutdown_timeout(Duration::from_secs(2)),
        )?;
        // Typed launcher arms parent-death before exec and closes its race.
        // The raw Child + setpriv candidate failed this gate and was removed.
        let (stdout, stdout_writer) = tokio::io::duplex(65536);
        let (stderr, stderr_writer) = tokio::io::duplex(65536);
        let (stdin, mut stdin_reader) = tokio::io::duplex(65536);
        let stop_output = processkit::CancellationToken::new();
        let mut command = processkit::Command::new(&launch.executable)
            .args(launch.args)
            .env_clear()
            .envs(launch.env)
            .keep_stdin_open()
            .kill_on_parent_death()
            .output_buffer(OutputBufferPolicy::bounded(0).with_max_bytes(65536))
            .stdout_raw_tee(pipe::ClosingWriter::new(stdout_writer, stop_output.clone()))
            .stderr_raw_tee(pipe::ClosingWriter::new(stderr_writer, stop_output.clone()));
        if let Some(directory) = launch.directory {
            command = command.current_dir(directory);
        }
        let mut child = group.start(&command).await?;
        let pid = child.pid();
        let mut sink = child.take_stdin().ok_or(Error::InvalidLaunch)?;
        // ProcessStdin has an async write API, not AsyncWrite. This byte bridge
        // preserves Crowsi framing without introducing another protocol. Its
        // task is owned and joined, never replayed, and cancelled on child exit.
        let input_task = tokio::spawn(async move {
            let mut bytes = [0; 8192];
            loop {
                let n = stdin_reader.read(&mut bytes).await?;
                if n == 0 {
                    return sink.finish().await;
                }
                sink.write(&bytes[..n]).await?;
            }
        });
        let input_abort = input_task.abort_handle();
        // Drive the raw stream pumps even while the caller is waiting on IPC.
        // Exactly one owned task per process; Drop aborts it, wait joins it.
        let waiter = tokio::spawn(async move {
            let result = child.drain().await;
            input_abort.abort();
            result
        });
        Ok(Self {
            group,
            waiter,
            input_task,
            pid,
            stdin: Some(stdin),
            stdout: Some(stdout),
            stderr: Some(stderr),
            exit: None,
            waiter_consumed: false,
            stop_output,
        })
    }
    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            containment: self.group.mechanism().name(),
            parent_death_scope: processkit::Command::kill_on_parent_death_scope().name(),
        }
    }
    pub fn pid_for_diagnostics(&self) -> Option<u32> {
        self.pid
    }
    pub fn take_stdin(&mut self) -> Option<DuplexStream> {
        self.stdin.take()
    }
    pub fn take_stdout(&mut self) -> Option<DuplexStream> {
        self.stdout.take()
    }
    pub fn take_stderr(&mut self) -> Option<DuplexStream> {
        self.stderr.take()
    }
    pub fn is_alive(&mut self) -> Result<bool, Error> {
        let members = self.group.members()?;
        Ok(!self.waiter.is_finished() && self.pid.is_some_and(|pid| members.contains(&pid)))
    }
    pub async fn wait(&mut self) -> Result<Outcome, Error> {
        if let Some(exit) = self.exit {
            return Ok(exit);
        }
        if self.waiter_consumed {
            return Err(Error::ReapTimeout);
        }
        let joined = (&mut self.waiter).await;
        // Even a failed JoinHandle is consumed. Never poll it again on a later
        // stop/inspection; cancellation before completion leaves this false.
        self.waiter_consumed = true;
        let status = joined.map_err(|_| Error::ReapTimeout)??;
        let _ = (&mut self.input_task).await;
        self.pid = None;
        // A leader exiting is not permission to leave descendants behind.
        self.group.kill_all()?;
        self.exit = Some(status);
        Ok(status)
    }
    pub async fn shutdown(&mut self, grace: Duration) -> Result<Outcome, Error> {
        if grace > Duration::from_secs(30) {
            return Err(Error::InvalidLaunch);
        }
        self.take_stdin();
        self.group.stop(grace, true).await?;
        // Application/Crowsi drain precedes this physical shutdown. Once the
        // tree has stopped, an abandoned reader cannot hold process reaping.
        self.stop_output.cancel();
        tokio::time::timeout(Duration::from_secs(2), self.wait())
            .await
            .map_err(|_| Error::ReapTimeout)?
    }
    /// Hard stop targets only this processkit-owned tree; callers must then reap.
    pub fn kill(&self) -> Result<(), Error> {
        self.group.kill_all()?;
        self.stop_output.cancel();
        Ok(())
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        self.waiter.abort();
        self.input_task.abort();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod failure_tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn failed_wait_is_terminal_and_repeat_shutdown_never_repolls_join() {
        let mut process = OwnedProcess::spawn(Launch::new("/bin/sleep").args(["60"]))
            .await
            .unwrap();
        process.waiter.abort();
        assert!(matches!(process.wait().await, Err(Error::ReapTimeout)));
        assert!(matches!(process.wait().await, Err(Error::ReapTimeout)));
        // Failed disposal remains typed failure, never fabricated successful reap.
        assert!(process.shutdown(Duration::ZERO).await.is_err());
    }
}

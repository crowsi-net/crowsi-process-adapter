//! Explicit systemd user-service backend for delegated, bounded local execution.
//! Environment values are PUBLIC launch configuration; secrets belong in stdin.
use crate::{Error, Launch, Outcome, OwnedProcess};
use std::{os::unix::fs::MetadataExt, time::Duration};
pub struct Service {
    process: OwnedProcess,
    unit: String,
    runtime: String,
}
impl Service {
    pub async fn spawn_public_environment(
        mut launch: Launch,
        lifetime: Duration,
    ) -> Result<Self, Error> {
        launch.validate()?;
        let budget = launch.budget.take().ok_or(Error::InvalidLaunch)?;
        if lifetime < Duration::from_millis(50) || lifetime > Duration::from_secs(120) {
            return Err(Error::InvalidLaunch);
        }
        // Require every whole-tree axis. systemd applies these before ExecStart.
        let memory = budget.max_memory_bytes.ok_or(Error::InvalidLaunch)?;
        let tasks = budget.max_processes.ok_or(Error::InvalidLaunch)?;
        let cpu = budget.cpu_quota.ok_or(Error::InvalidLaunch)?;
        let file = budget.max_file_bytes.ok_or(Error::InvalidLaunch)?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).map_err(|_| Error::InvalidLaunch)?;
        let unit = format!(
            "crowsi-{}.service",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let uid = std::fs::metadata("/proc/self")
            .map_err(|_| Error::InvalidLaunch)?
            .uid();
        let runtime = format!("/run/user/{uid}");
        let mut args = vec![
            "--user".into(),
            "--quiet".into(),
            "--wait".into(),
            "--pipe".into(),
            "--collect".into(),
            "--service-type=exec".into(),
            "--expand-environment=no".into(),
            format!("--unit={unit}").into(),
            format!("--property=MemoryMax={memory}").into(),
            "--property=MemorySwapMax=0".into(),
            format!("--property=TasksMax={tasks}").into(),
            format!("--property=CPUQuota={}%", cpu * 100.0).into(),
            format!("--property=LimitFSIZE={file}").into(),
            "--property=LimitCORE=0".into(),
            format!("--property=RuntimeMaxSec={}ms", lifetime.as_millis()).into(),
            "--property=TimeoutStopSec=1s".into(),
            "--property=KillMode=control-group".into(),
            "--property=Restart=no".into(),
            "--property=UMask=0077".into(),
        ];
        if let Some(directory) = launch.directory.take() {
            args.push(
                format!(
                    "--working-directory={}",
                    directory.to_str().ok_or(Error::InvalidLaunch)?
                )
                .into(),
            );
        }
        args.extend(["--".into(), "/usr/bin/env".into(), "-i".into()]);
        for (key, value) in launch.env {
            let key = key.to_str().ok_or(Error::InvalidLaunch)?;
            let value = value.to_str().ok_or(Error::InvalidLaunch)?;
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(Error::InvalidLaunch);
            }
            args.push(format!("{key}={value}").into());
        }
        args.push(launch.executable.into_os_string());
        args.extend(launch.args);
        let process = OwnedProcess::spawn(
            Launch::new("/usr/bin/systemd-run")
                .args(args)
                .env("XDG_RUNTIME_DIR", &runtime),
        )
        .await?;
        Ok(Self {
            process,
            unit,
            runtime,
        })
    }
    pub fn take_stdin(&mut self) -> Option<tokio::io::DuplexStream> {
        self.process.take_stdin()
    }
    pub fn take_stdout(&mut self) -> Option<tokio::io::DuplexStream> {
        self.process.take_stdout()
    }
    pub fn take_stderr(&mut self) -> Option<tokio::io::DuplexStream> {
        self.process.take_stderr()
    }
    pub async fn wait(&mut self) -> Result<Outcome, Error> {
        let result = self.process.wait().await;
        self.stop().await?;
        result
    }
    async fn stop(&self) -> Result<(), Error> {
        let mut stop = OwnedProcess::spawn(
            Launch::new("/usr/bin/systemctl")
                .args(["--user", "stop", "--", &self.unit])
                .env("XDG_RUNTIME_DIR", &self.runtime),
        )
        .await?;
        match tokio::time::timeout(Duration::from_secs(3), stop.wait()).await {
            Ok(r) => {
                r?;
                Ok(())
            }
            Err(_) => {
                stop.shutdown(Duration::ZERO).await?;
                Err(Error::ReapTimeout)
            }
        }
    }
    pub async fn shutdown(&mut self, grace: Duration) -> Result<Outcome, Error> {
        self.stop().await?;
        self.process.shutdown(grace).await
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        // Best effort cancellation on ordinary drop. RuntimeMaxSec remains active
        // even if the host crashes or its async runtime is already gone.
        let unit = self.unit.clone();
        let runtime = self.runtime.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Ok(mut p) = OwnedProcess::spawn(
                    Launch::new("/usr/bin/systemctl")
                        .args(["--user", "stop", "--", &unit])
                        .env("XDG_RUNTIME_DIR", runtime),
                )
                .await
                    && tokio::time::timeout(Duration::from_secs(3), p.wait())
                        .await
                        .is_err()
                {
                    let _ = p.shutdown(Duration::ZERO).await;
                }
            });
        }
    }
}

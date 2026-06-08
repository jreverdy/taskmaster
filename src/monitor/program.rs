use super::parsing::Config;
use std::{
    error::Error,
    fs::File,
    process::{Command, Stdio},
};

pub struct Program {
    pub config: Config,
    pub command: Option<Command>,
    active: bool,
}

impl Program {
    pub fn new(config: Config, command: Option<Command>, active: bool) -> Self {
        Self {
            config,
            command,
            active,
        }
    }

    pub fn build_command(&mut self) -> Result<(), Box<dyn Error>> {
        let mut parts = self.config.cmd.split_whitespace();
        let program_name = parts.next().ok_or("Missing program name")?;

        let mut cmd = Command::new(program_name);
        cmd.args(parts)
            .envs(self.config.env.iter())
            .current_dir(&self.config.workingdir);

        let output = self
            .fd_setup()
            .map_err(|err| format!("Failed to parse std's: {err}"))?;

        cmd.stdout(output.0)
           .stderr(output.1);

        self.command = Some(cmd);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn deactivate(&mut self) {
        self.active = false;
    }

    pub fn activate(&mut self) {
        self.active = true;
    }

    pub fn prefix_name(prefix: &str, name: String) -> String {
        format!("{prefix}{name}")
    }

    fn fd_setup(&self) -> Result<(Stdio, Stdio), Box<dyn Error>> {
        let stdout = if self.config.stdout.as_os_str().is_empty() {
            Stdio::null()
        } else {
            let s = self.config.workingdir.join(&self.config.stdout);
            Stdio::from(
                File::create(&s).map_err(|err| format!("stdout = '{}' {}", s.display(), err))?,
            )
        };
        let stderr = if self.config.stderr.as_os_str().is_empty() {
            Stdio::null()
        } else {
            let s = self.config.workingdir.join(&self.config.stderr);
            Stdio::from(
                File::create(&s).map_err(|err| format!("stderr = '{}' {}", s.display(), err))?,
            )
        };

        Ok((stdout, stderr))
    }
}

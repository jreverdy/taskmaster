use serde::{Deserialize, Deserializer};
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::{collections::HashMap, fs};

use crate::monitor::program::Program;
use crate::signal::Signal;

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AutoRestart {
    Always,
    #[default]
    Never,
    Unexpected,
}

impl AutoRestart {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Never => "never",
            Self::Unexpected => "unexpected",
        }
    }
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub cmd: String,
    pub numprocs: usize,
    #[serde(deserialize_with = "umask_deserialize")]
    pub umask: u32,
    #[serde(deserialize_with = "workingdir_deserialize")]
    pub workingdir: PathBuf,
    pub autostart: bool,
    pub autorestart: AutoRestart,
    pub exitcodes: Vec<i32>,
    pub startretries: usize,
    pub starttime: usize,
    #[serde(deserialize_with = "signal_deserialize")]
    pub stopsignal: Signal,
    pub stoptime: usize,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub env: HashMap<String, String>,
}

fn umask_deserialize<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = String::deserialize(deserializer)?;
    u32::from_str_radix(&buf, 8).map_err(serde::de::Error::custom)
}

fn signal_deserialize<'de, D>(deserializer: D) -> Result<Signal, D::Error>
where
    D: Deserializer<'de>,
{
    let buf: String = String::deserialize(deserializer)?;
    Signal::from_str(&buf).map_err(serde::de::Error::custom)
}

fn workingdir_deserialize<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let buf = PathBuf::deserialize(deserializer)?;

    if !buf.is_dir() {
        Err(serde::de::Error::custom(format!(
            "Invalid working directory: {}",
            buf.display()
        )))
    } else {
        Ok(buf)
    }
}

#[derive(Deserialize)]
pub struct Parsing {
    #[serde(flatten)]
    pub tasks: HashMap<String, Config>,
}

impl Parsing {
    pub fn parse<P: AsRef<Path>>(file_path: P) -> Result<HashMap<String, Program>, Box<dyn Error>> {
        let file_content = fs::read_to_string(file_path)?;
        let parsed: Parsing = serde_yaml::from_str(&file_content)?;

        Ok(parsed.tasks
            .into_iter()
            .map(|(name, config)| (name, Program::new(config, None, true)))
            .collect())
    }
}

use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process;

pub fn get_config_from_args() -> PathBuf {
    let args: Vec<String> = env::args().collect();
    let mut path: PathBuf = PathBuf::new();
    match args.len() {
        2 => path.push(&args[1]),
        3.. => {
            eprintln!("Taskmaster: Too many arguments");
            process::exit(1);
        }
        _ => {
            eprintln!("Taskmaster: Missing config file name");
            process::exit(1);
        }
    };

    if let Err(err) = check_file_with_extension(&path, "yaml") {
        eprintln!("Taskmaster: Config path: {err}");
        process::exit(1);
    }

    path
}

pub fn check_valid_path(path: &Path) -> Result<(), Box<dyn Error>> {
    match path.try_exists() {
        Ok(true) => Ok(()),
        _ => Err("Path does not point to an existing entity".into()),
    }
}

pub fn check_is_file(path: &Path) -> Result<(), Box<dyn Error>> {
    check_valid_path(path)?;
    if !path.is_file() {
        Err("File does not exist or is not accessible".into())
    } else {
        Ok(())
    }
}

pub fn check_file_with_extension(path: &Path, extension: &str) -> Result<(), Box<dyn Error>> {
    check_is_file(path)?;
    match path.extension() {
        Some(x) => {
            if x != extension {
                Err("Wrong file extension")
            } else {
                Ok(())
            }
        }
        None => Err("Failed to retrieve file extension"),
    }?;
    Ok(())
}

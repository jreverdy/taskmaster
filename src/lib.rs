pub mod monitor;
pub mod signal;
pub mod channel;
mod sys;

use monitor::*;
use monitor::instruction::*;
use std::sync::mpsc::{self, Sender, Receiver};
use std::{thread};
use std::fmt::Write;
use std::error::Error;
use std::path::PathBuf;
use rustyline::{Editor, history::DefaultHistory};


use rustyline::completion::{Completer, Pair};
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, ExternalPrinter, Helper};
use rustyline::error::ReadlineError;

use crate::channel::{ChannelResponse, ProgramStatus};

const COMMANDS: &[&str] = &[
    "start",
    "stop",
    "restart",
    "status",
    "reload",
    "exit",
];

struct CmdHelper;

impl Helper for CmdHelper {}
impl Hinter for CmdHelper {
    type Hint = String;
}
impl Highlighter for CmdHelper {}
impl Validator for CmdHelper {}

impl Completer for CmdHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        _: usize,
        _: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {

        let matches = COMMANDS
            .iter()
            .filter(|cmd| cmd.starts_with(line))
            .map(|cmd| Pair {
                display: cmd.to_string(),
                replacement: cmd.to_string(),
            })
            .collect();

        Ok((0, matches))
    }
}

pub struct Taskmaster {
    config_file_path: PathBuf,
}

impl Taskmaster {
    pub fn new(file_path: PathBuf) -> Result<Self, Box<dyn Error>> {
        Ok(Taskmaster {
            config_file_path: file_path,
        })
    }

    pub fn execute(mut self) -> Result<(), Box<dyn Error>> {
        let (sender, receiver) = mpsc::channel::<Instruction>();
        let (sender_result, receiver_response) = mpsc::channel::<ChannelResponse>();
        let mut monitor = Monitor::new(&self.config_file_path)?;
        thread::spawn(move || {
            monitor.execute(receiver, sender_result);
        });
        self.cli(sender, receiver_response);
        Ok(())
    }


    fn cli(&mut self, sender: Sender<Instruction>, receiver: Receiver<ChannelResponse>) {
        let mut rl = Editor::<CmdHelper, DefaultHistory>::new().unwrap();
        rl.set_helper(Some(CmdHelper));

        let printer = rl.create_external_printer().unwrap();

        Taskmaster::receive_and_print_response(printer, receiver);

        loop {
            let line = rl.readline("Taskmaster $> ");
            
            let line: String = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            if line.trim().is_empty() { continue; }

            rl.add_history_entry(line.as_str()).ok();

            let instruction: Instruction = match line.trim().parse() {
                Ok(res) => res,
                Err(err) => {
                    eprintln!("{err}");
                    continue;
                }
            };
            
            if sender.send(instruction).is_err() {
                eprintln!("Failed to send instruction to backend");
            }
        }
    }

    pub fn receive_and_print_response<P>(mut printer: P, receiver: Receiver<ChannelResponse>)
    where 
        P: ExternalPrinter + Send + 'static 
    {
        thread::spawn(move || {
            let p_green = "\x1b[38;2;161;212;161m";
            let p_orange = "\x1b[38;2;255;187;119m";
            let reset = "\x1b[0m";

            while let Ok(response) = receiver.recv() {
                let output = match response {
                    ChannelResponse::Status(statuses) => {
                        format!("{}\n", Taskmaster::format_status_result(statuses))
                    }
                    ChannelResponse::Error(err) => {
                        format!("    {p_orange}Error: {err}{reset}\n")
                    }
                    ChannelResponse::Feedback(feedback) => {
                        format!("    {p_green}{feedback}{reset}\n")
                    }
                };

                let _ = printer.print(output);
            }
        });
    }

    pub fn format_status_result(statuses: Vec<ProgramStatus>) -> String {
        let mut buffer = String::new();
        
        let _ = writeln!(buffer, "\n    {:<5} | {:<20} | {:<10}", "ID", "NAME", "STATUS");
        let _ = writeln!(buffer, "    {:-<50}", "");

        for status in statuses {
            let _ = writeln!(buffer, "    {:<5} | {:<20} | {:<10}", 
                status.id, 
                status.name, 
                status.status
            );
        }

        buffer
    }
}
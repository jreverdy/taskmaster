pub mod processus;
pub mod program;
pub mod logger;
pub mod instruction;
pub mod parsing;

use crate::{
    channel::{ChannelResponse, ProgramStatus},
    signal::Signal,
    sys::{self, Libc},
};
use instruction::Instruction;
use logger::Logger;
use parsing::Parsing;
use processus::{id::Id, Processus, Status};
use program::Program;
use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    os::unix::process::ExitStatusExt,
    path::PathBuf,
    process::{self, ExitStatus},
    sync::{
        atomic::Ordering,
        mpsc::{Receiver, Sender},
    },
    thread,
    time::Duration,
    vec,
};

const INACTIVE_FLAG: &str = "Inactive";

fn hup_signal_handler(_: i32) {
    sys::RELOAD_INSTRUCTION.store(true, Ordering::SeqCst);
}

pub struct Monitor {
    config_file_path: PathBuf,
    processus: Vec<Processus>,
    logger: Logger,
    programs: HashMap<String, Program>,
}

impl Monitor {
    pub fn new(file_path: &PathBuf) -> Result<Self, Box<dyn Error>> {
        let mut programs = Parsing::parse(file_path)?;
        let mut logger = Logger::new("taskmaster.log")?;
        let mut processus: Vec<Processus> = Vec::new();

        let mut invalid_confs = Vec::<String>::new();
        for (name, program) in programs.iter_mut() {
            if let Err(err) = program.build_command() {
                eprintln!("Program {name}: {err}");
                logger.log(&format!("Failed to build command for program {name}: {err}"));
                invalid_confs.push(name.to_owned());
                continue;
            }
            for _ in 0..program.config.numprocs {
                processus.push(Processus::new(name, program));
            }
        }
        for name in &invalid_confs {
            programs.remove(name);
        }
        
        Ok(Monitor {
            config_file_path: file_path.to_owned(),
            processus,
            logger,
            programs,
        })
    }

    pub fn capture_signal() {
        if Libc::signal(Signal::SIGHUP, hup_signal_handler).is_err() {
            eprintln!("Signal function failed, taskmaster won't be able to handle SIGHUP");
        }
    }

    pub fn execute(&mut self, receiver: Receiver<Instruction>, mut sender_result: Sender<ChannelResponse>) {
        Self::capture_signal();
        self.autostart(&mut sender_result);

        let mut instruction_queue: VecDeque<Instruction> = VecDeque::new();
        
        loop {
            if sys::RELOAD_INSTRUCTION.load(Ordering::SeqCst) {
                instruction_queue.push_front(Instruction::Reload);
                sys::RELOAD_INSTRUCTION.store(false, Ordering::SeqCst);
            } else if sys::QUIT_INSTRUCTION.load(Ordering::SeqCst) {
                instruction_queue.push_front(Instruction::Exit);
            }
            if let Ok(instruction) = receiver.try_recv() {
                instruction_queue.push_back(instruction);
            }
            while let Some(instruction) = instruction_queue.pop_front() {
                match instruction {
                    // Instruction from cli
                    Instruction::Status => self.status_command(&mut sender_result),
                    Instruction::Start(programs) => self.start_command(programs, &mut sender_result),
                    Instruction::Stop(programs) => self.stop_command(programs, &mut sender_result),
                    Instruction::Restart(programs) => self.restart_command(programs, &mut sender_result),
                    Instruction::Reload => self.reload(&mut sender_result),
                    Instruction::Exit => self.exit_command(&mut sender_result),
                    // Instruction not from Cli
                    Instruction::RemoveProcessus(id) => self.remove_processus(id, &mut sender_result),
                    Instruction::StartProcessus(id) => self.start_processus(id, &mut sender_result),
                    Instruction::ResetProcessus(id) => self.reset_processus(id),
                    Instruction::RetryStartProcessus(id) => self.start_processus(id, &mut sender_result),
                    Instruction::SetStatus(id, status) => self.set_status(id, status),
                    Instruction::KillProcessus(id) => self.kill_processus(id),
                }
            }
            instruction_queue.extend(self.monitor());
            thread::sleep(Duration::from_millis(300));
        }
    }
}

impl Monitor {

    fn get_processus(processus: &mut [Processus], id: Id) -> Option<&mut Processus> {
        processus.iter_mut().find(|processus| processus.id == id)
    }

    fn kill_processus(&mut self, id: Id) {
        if let Some(processus) = Self::get_processus(&mut self.processus, id) {
            if let Some(child) = &mut processus.child {
                child.kill().ok();
            }
            processus.child = None;
            if processus.status != Status::Reloading {
                processus.status = Status::Inactive;
            }
            self.logger.log(&format!("Sigkill processus {} {}", processus.name, processus.id));
        }
    }

    fn set_status(&mut self, id: Id, status: Status) {
        if let Some(processus) = Self::get_processus(&mut self.processus, id) {
            processus.status = status;
            self.logger.log(&format!("Setting status of processus {} {} to Active", processus.name, processus.id));
        }
    }

    fn start_processus(&mut self, id: Id, sender_result: &mut Sender<ChannelResponse>) {
        if let Some(processus) = Self::get_processus(&mut self.processus, id) {
            if let Some(program) = self.programs.get_mut(&processus.name) {
                if let Some(command) = &mut program.command {
                    match processus.start_child(
                        command,
                        program.config.startretries,
                        program.config.umask,
                    ) {
                        Ok(false) => {
                            self.logger.log(&format!(
                                "Starting processus {} {}, {} attempt left", processus.name, processus.id, processus.retries)
                            );
                        }
                        Ok(true) => {
                            self.logger.log(&format!("Failed to start processus {} {}, no attempt left", processus.name, processus.id)
                        );
                        }
                        Err(err) => {
                            let msg = format!("{err}");

                            let _ = sender_result.send(
                                ChannelResponse::Feedback(msg.clone())
                            );

                            self.logger.log(&msg);
                        }
                    }
                } else {
                    eprintln!(
                        "Can't find command to start processus {} {}",
                        processus.name,
                        processus.id
                    );
                    self.logger.log(&format!("Can't find command to start processus {} {}", processus.name, processus.id));
                }
            } else {
                eprintln!(
                    "Can't find program to start processus {} {}",
                    processus.name,
                    processus.id
                );
                self.logger.log(&format!("Can't find program to start processus {} {}", processus.name, processus.id));
            }
        }
    }

    fn reset_processus(&mut self, id: Id) {
        if let Some(processus) = Self::get_processus(&mut self.processus, id) {
            if let Some(program) = self.programs.get(&processus.name) {
                self.logger.log(&format!("Reset processus {} {}", processus.name, processus.id));
                processus.reset_child(program.config.startretries)
            }
        }
    }

    fn remove_processus(&mut self, id: Id, sender: &mut Sender<ChannelResponse>) {
        if let Some(processus) = Self::get_processus(&mut self.processus, id) {
            let processus_name = processus.name.to_owned();
            self.processus.retain(|proc| proc.id != id);
            if !self.processus.iter().any(|e| e.name == processus_name) {
                self.programs.remove(&processus_name);
                let name = if let Some((name, _)) = self.programs.iter().find(|e| e.0 == &[INACTIVE_FLAG, &processus_name].concat()) {
                    name.to_owned()
                } else {
                    return;
                };
                if let Some(mut program) = self.programs.remove(&name) {
                    program.activate();
                    self.programs.insert(processus_name.to_owned(), program);
                    let program = self.programs.get(&processus_name).unwrap();
                    for _ in 0..program.config.numprocs {
                        self.processus.push(Processus::new(&processus_name, program));
                    }
                    if program.config.autostart {
                        self.start_programs(vec![processus_name], sender);
                    }
                }
            }
        }
    }

    fn monitor_active_processus(program: &Program, processus: &Processus, exit_code: Option<ExitStatus>) -> Option<Instruction> {
        match exit_code {
            Some(code) => {
                match program.config.autorestart.as_str() {
                    "always" => {Some(Instruction::StartProcessus(processus.id))},
                    "never" => {Some(Instruction::ResetProcessus(processus.id))},
                    "unexpected" => {
                        match code.code() {
                            Some(code) if program.config.exitcodes.contains(&code) => Some(Instruction::ResetProcessus(processus.id)),
                            _ => Some(Instruction::StartProcessus(processus.id)),
                        }
                    },
                    _ => {panic!("autorestart has an invalid value");}
                }
            },
            _ => {None},
        }
    }

    fn monitor_inactive_processus(processus: &Processus) {
        panic!("Child exist but the processus {} {} status is Inactive", processus.id, processus.name);
    }

    fn monitor_starting_processus(program: &Program, processus: &Processus, exit_code: Option<ExitStatus>) -> Option<Instruction> {
        match exit_code {
            Some(_) => {
                if processus.retries > 0 {
                    Some(Instruction::RetryStartProcessus(processus.id))
                } else {
                    Some(Instruction::ResetProcessus(processus.id))
                }
            },
            None => {
                if processus.is_timeout(program.config.starttime) {
                    Some(Instruction::SetStatus(processus.id, Status::Active))
                } else {
                    None
                }
            },
        }
    }

    fn monitor_stopping_processus(program: &Program, processus: &Processus, exit_code: Option<ExitStatus>) -> Option<Instruction> {
        match exit_code {
            Some(_) => Some(Instruction::ResetProcessus(processus.id)),
            None => {
                if processus.is_timeout(program.config.stoptime) {
                    Some(Instruction::KillProcessus(processus.id))
                } else {
                    None
                }
            }
        }
    }

    fn monitor_remove_processus(program: &Program, processus: &Processus, exit_code: Option<ExitStatus>) -> Option<Instruction> {
        match exit_code {
            Some(_) => {
                Some(Instruction::RemoveProcessus(processus.id))
            }
            None => {
                if processus.is_timeout(program.config.stoptime) {
                    Some(Instruction::KillProcessus(processus.id))
                } else {
                    None
                }
            }
        }
    }

    fn monitor_processus(program: &Program, processus: &Processus, exit_code: Option<ExitStatus>) -> Option<Instruction> {
        match processus.status {
            Status::Active => Self::monitor_active_processus(program, processus, exit_code),
            Status::Inactive => {Self::monitor_inactive_processus(processus); None},
            Status::Starting => Self::monitor_starting_processus(program, processus, exit_code),
            Status::Stopping => Self::monitor_stopping_processus(program, processus, exit_code),
            Status::Reloading => Self::monitor_remove_processus(program, processus, exit_code),
        }
    }

    fn monitor(&mut self) -> Vec<Instruction> {
        let mut instructions = Vec::new();

        for processus in self.processus.iter_mut() {
            if let Some(child) = processus.child.as_mut() {
                match child.try_wait() {
                    Err(_) => panic!("Try_wait failed on processus {} {}", processus.id, processus.name),
                    Ok(code) => {
                        if let Some(code) = code {
                            if let Some(signal) = code.signal() {
                                if processus.status != Status::Reloading {
                                    self.logger.log(&format!("Processus {} {} was stopped by a signal: {}", processus.name, processus.id, signal));
                                }
                        } else if let Some(exit_code) = code.code() {
                            let program = self.programs.get(&processus.name).unwrap();
                            let expected = if program.config.exitcodes.contains(&exit_code) { "expected" } else { "unexpected" };
                            self.logger.log(&format!("Processus {} {} exited with code {} ({})", processus.name, processus.id, exit_code, expected));
                            }
                        }
                        if let Some(instruction) = Self::monitor_processus(self.programs.get(&processus.name).unwrap(), processus, code) {
                            instructions.push(instruction);
                        }
                    },
                };
            } else if processus.status == Status::Reloading {
                instructions.push(Instruction::RemoveProcessus(processus.id));
            }
        }
        instructions
    }

    fn status_command(&mut self, sender_result: &mut Sender<ChannelResponse>) {
        let statuses: Vec<ProgramStatus> = self.processus
            .iter()
            .map(|processus| ProgramStatus {
                id: processus.id.to_string(),
                name: processus.name.clone(),
                status: processus.status.to_string(),
            })
            .collect();

        sender_result.send(ChannelResponse::Status(statuses)).ok();

        self.logger.log("Displaying Status");
    }

    fn start_command(&mut self, names: Vec<String>, sender_result: &mut Sender<ChannelResponse>) {
        for name in names {
            if self.programs.get_mut(&name).is_none() {
                sender_result.send(ChannelResponse::Error(format!("Program not found: {name}"))).ok();
                continue;
            }
            let filtered_processus_ids: Vec<Id> = self.processus
                .iter()
                .filter(|e| e.name == name && e.status == Status::Inactive)
                .map(|e| e.id)
                .collect();
            for pid in filtered_processus_ids {
                self.start_processus(pid, sender_result);
            }
            sender_result.send(ChannelResponse::Feedback(format!("Program {name} started"))).ok();
            self.logger.log(&format!("Starting program {}", &name));
        }
    }

    fn stop_command(&mut self, names: Vec<String>, sender_result: &mut Sender<ChannelResponse>) {
        for name in names {
            let program = if let Some(program) = self.programs.get_mut(&name) {
                program
            } else {
                sender_result.send(ChannelResponse::Error(format!("Program not found: {name}"))).ok();
                continue;
            };
            for processus in self.processus.iter_mut().filter(|e| e.name == name) {
                Self::stop_processus(processus, program, &mut self.logger);
            }
            sender_result.send(ChannelResponse::Feedback(format!("Program {name} stopped"))).ok();
            self.logger.log(&format!("Stopping {}", &name));
        }
    }

    fn stop_processus(processus: &mut Processus, program: &mut Program, logger: &mut Logger) {
        if let Some(child) = processus.child.as_mut() {
            match child.try_wait() {
                Ok(Some(exitstatus)) => {
                    let msg = format!("The program {} is already stopped, exit code : {exitstatus}", processus.name);
                    println!("{msg}");
                    logger.log(&msg);
                },
                Ok(None) => {
                    if let Err(err) = processus.stop_child(program.config.stopsignal, program.config.startretries) {
                        let msg = format!("Failed to stop program {}: {}", processus.name, err);
                        eprintln!("{msg}");
                        logger.log(&msg);
                    }
                }
                Err(_) => {
                    panic!("try_wait() failed");
                },
            };
        }
    }

    fn restart_command(&mut self, names: Vec<String>, sender_result: &mut Sender<ChannelResponse>) {
        let send_result = match self.restart_programs(names, sender_result) {
            Some(err) => sender_result.send(ChannelResponse::Error(err.to_string())),
            None => {
                self.logger.log("Restarting programs");
                sender_result.send(ChannelResponse::Feedback(
                    "Programs restarted successfully".to_string()
                ))
            }
        };
        if send_result.is_err() {
            self.logger.log(&format!("Failed to send restart command result: {send_result:?}"));
        }
        
    }

    fn restart_programs(&mut self, names: Vec<String>, sender: &mut Sender<ChannelResponse>) -> Option<Box<dyn Error>> {
        for name in &names {
            if self.programs.get(name).is_none() {
                return Some(format!("Program not found: {name}").into());
            }
        }

        if let Some(err) = self.stop_programs(names.to_owned()) {
            return Some(err);
        }
        if let Some(err) = self.start_programs(names.to_owned(), sender) {
            return Some(err);
        }
        None
    }

    fn start_programs(&mut self, names: Vec<String>, sender_result: &mut Sender<ChannelResponse>) -> Option<Box<dyn Error>> {
        for name in names {
            if self.programs.get_mut(&name).is_none() {
                return Some(format!("Program not found: {name}").into());
            }

            let filtered_processus_ids: Vec<Id> = self.processus
                .iter()
                .filter(|e| e.name == name && e.status == Status::Inactive)
                .map(|e| e.id)
                .collect();

            for pid in filtered_processus_ids {
                self.start_processus(pid, sender_result);
            }

            self.logger.log(&format!("Starting program {}", &name));
        }

        None
    }

    fn stop_programs(&mut self, names: Vec<String>) -> Option<Box<dyn Error>> {
        for name in names {
            let program = match self.programs.get_mut(&name) {
                Some(p) => p,
                None => return Some(format!("Program not found: {name}").into()),
            };

            for processus in self.processus.iter_mut().filter(|e| e.name == name) {
                Self::stop_processus(processus, program, &mut self.logger);
            }

            self.logger.log(&format!("Stopping {}", &name));
        }

        None
    }


    fn autostart(&mut self, sender_result: &mut Sender<ChannelResponse>) {
        let mut to_start: Vec<String> = Vec::new();
        for (name, program) in self.programs.iter() {
            if program.config.autostart {
                self.logger.log(&format!("Autostart {name}"));
                to_start.push(name.to_owned());
            }
        }
        if let Some(err) = self.start_programs(to_start, sender_result) {
            self.logger.log(&format!("Failed to autostart some programs: {err}"));  
        }
    }

    fn exit_command(&mut self, sender_result: &mut Sender<ChannelResponse>) {
        let mut to_stop = Vec::new();
        self.logger.log("Shutting down taskmaster");
        let _ = sender_result.send(
            ChannelResponse::Feedback("Waiting every programs to quit before exiting...".to_string())
        );
        for (name, _) in self.programs.iter() {
            to_stop.push(name.to_owned());
        }
        self.stop_programs(to_stop);
        while self.processus.iter().any(|e| e.child.is_some()) {
            for instruction in self.monitor() {
                match instruction {
                    Instruction::ResetProcessus(id) => self.reset_processus(id),
                    Instruction::KillProcessus(id) => self.kill_processus(id),
                    _ => {}
                }
            }
        }
        process::exit(0);
    }

    fn clear_removed_programs(&mut self, new_config: &HashMap<String, Program>) {
        let to_remove: Vec<String> = self.programs.keys()
            .filter(|name| !name.starts_with(INACTIVE_FLAG) && !new_config.contains_key(*name))
            .cloned()
            .collect();

        for name in to_remove {
            self.stop_programs(vec![name.clone()]);
            self.programs.remove(&name);
            self.processus.retain(|p| p.name != name);
            self.logger.log(&format!("Program {} removed (no longer in config)", name));
        }
    }

    fn update_program(&mut self, name: String, mut program: Program, sender: &mut Sender<ChannelResponse>) {
        if let Err(err) = program.build_command() {
            let _ = sender.send(ChannelResponse::Error(format!("Program {name}: {err}")));
            self.logger.log(&format!("Failed to build command for updated program {name}: {err}"));
            return;
        }

        self.stop_programs(vec![name.clone()]);

        self.processus.iter_mut()
            .filter(|p| p.name == name)
            .for_each(|p| p.status = Status::Reloading);

        program.deactivate();
        
        let inactive_key = Program::prefix_name(INACTIVE_FLAG, name.clone());
        self.logger.log(&format!("Program {} updated (config changed)", name));
        self.programs.insert(inactive_key, program);
    }

    fn add_program(&mut self, name: String, mut program: Program, sender: &mut Sender<ChannelResponse>) {
        if let Err(err) = program.build_command() {
            let _ = sender.send(ChannelResponse::Error(format!("Program {name}: {err}")));
            self.logger.log(&format!("Failed to build command for new program {name}: {err}"));
            return;
        }

        for _ in 0..program.config.numprocs {
            self.processus.push(Processus::new(&name, &program));
        }

        self.logger.log(&format!("Program {} added (new in config)", name));
        let autostart = program.config.autostart;
        self.programs.insert(name.clone(), program);

        if autostart {
            self.start_programs(vec![name], sender);
        }
    }

    fn reload(&mut self, sender_result: &mut Sender<ChannelResponse>) {
        self.logger.log("Reloading config file");

        let mut new_programs = match Parsing::parse(&self.config_file_path) {
            Ok(p) => p,
            Err(err) => {
                let msg = format!("Failed to reload config file: {err}");
                self.logger.log(&msg);
                let _ = sender_result.send(ChannelResponse::Error(msg));
                return;
            }
        };

        self.clear_removed_programs(&new_programs);

        for (name, program) in new_programs.drain() {
            match self.programs.get(&name) {
                Some(old_program) => {
                    if old_program.config != program.config {
                        self.update_program(name, program, sender_result);
                    }
                }
                None => {
                    self.add_program(name, program, sender_result);
                }
            }
        }

        self.logger.log("Config reloaded successfully");
        let _ = sender_result.send(ChannelResponse::Feedback("Config reloaded successfully".to_owned()));
    }
}

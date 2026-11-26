//! The listening end of `obsbot_core::app_ipc`.

use crate::commands::{self, Command};
use obsbot_core::app_ipc;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the listener knows about the app, kept current by the main loop.
#[derive(Default)]
pub struct Shared {
    pub status: String,
    pub profiles: Vec<String>,
    pub positions: Vec<String>,
}

/// Names are matched without regard to case; `Err` lists the valid ones.
fn check_name(kind: &str, wanted: &str, known: &[String]) -> Result<(), String> {
    let wanted = wanted.trim().to_lowercase();
    if known.iter().any(|name| name.to_lowercase() == wanted) {
        return Ok(());
    }
    if known.is_empty() {
        Err(format!("there are no {kind}s yet"))
    } else {
        Err(format!(
            "no {kind} called '{}'. Known: {}",
            wanted,
            known.join(", ")
        ))
    }
}

/// Start listening. Returns `false` when another control center already is.
pub fn serve(commands: Sender<Command>, status: Arc<Mutex<Shared>>) -> std::io::Result<bool> {
    let path = app_ipc::socket_path();
    if app_ipc::send("status").is_ok() {
        return Ok(false);
    }
    // No one answered, so any socket file there is left over from a crash.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::thread::Builder::new()
        .name("app-commands".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &commands, &status);
            }
        })?;
    Ok(true)
}

fn answer(stream: UnixStream, commands: &Sender<Command>, shared: &Mutex<Shared>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    let mut reader = BufReader::new(&stream);
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let reply = match line.trim() {
        "status" => shared
            .lock()
            .map(|shared| shared.status.clone())
            .unwrap_or_default(),
        "help" => commands::HELP.to_owned(),
        text => {
            let checked = commands::parse(text).and_then(|command| {
                // Catch misspelt names here, so the caller hears about them.
                if let Ok(shared) = shared.lock() {
                    match &command {
                        Command::Profile(name) => check_name("profile", name, &shared.profiles)?,
                        Command::Position(name) => check_name("position", name, &shared.positions)?,
                        _ => {}
                    }
                }
                Ok(command)
            });
            match checked {
                Ok(command) => match commands.send(command) {
                    Ok(()) => "ok".to_owned(),
                    Err(_) => "error: the control center is shutting down".to_owned(),
                },
                Err(problem) => format!("error: {problem}"),
            }
        }
    };
    let mut stream = &stream;
    let _ = stream.write_all(reply.replace('\n', "; ").as_bytes());
    let _ = stream.write_all(b"\n");
}

/// Remove the socket file on a clean exit.
pub fn close() {
    let _ = std::fs::remove_file(app_ipc::socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_checked_without_regard_to_case() {
        let known = vec!["Meeting".to_owned(), "Late night".to_owned()];
        assert!(check_name("profile", " late NIGHT ", &known).is_ok());
        let problem = check_name("profile", "Nope", &known).unwrap_err();
        assert!(problem.contains("Known: Meeting, Late night"));
        assert!(check_name("position", "Desk", &[])
            .unwrap_err()
            .contains("no positions yet"));
    }
}

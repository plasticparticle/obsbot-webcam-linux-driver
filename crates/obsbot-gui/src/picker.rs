//! The desktop's own file chooser, opened without blocking the UI.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

#[derive(Default)]
pub struct FilePicker {
    pending: Option<Receiver<Option<PathBuf>>>,
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

impl FilePicker {
    /// Whether a supported chooser (`zenity` or `kdialog`) is installed.
    pub fn available() -> bool {
        on_path("zenity") || on_path("kdialog")
    }

    /// Open the chooser. `patterns` are globs such as `*.cube`.
    pub fn open(&mut self, title: &str, kind: &str, patterns: &[&str]) {
        let globs = patterns.join(" ");
        let (program, args) = if on_path("zenity") {
            (
                "zenity",
                vec![
                    "--file-selection".to_owned(),
                    format!("--title={title}"),
                    format!("--file-filter={kind} | {globs}"),
                ],
            )
        } else if on_path("kdialog") {
            (
                "kdialog",
                vec![
                    "--title".to_owned(),
                    title.to_owned(),
                    "--getopenfilename".to_owned(),
                    ".".to_owned(),
                    format!("{kind} ({globs})"),
                ],
            )
        } else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        std::thread::spawn(move || {
            let output = Command::new(program)
                .args(args)
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output();
            let picked = output.ok().and_then(|output| {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                (output.status.success() && !text.is_empty()).then(|| PathBuf::from(text))
            });
            let _ = tx.send(picked);
        });
    }

    /// True while the chooser is open.
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }

    /// The chosen file, once the user has picked one.
    pub fn take(&mut self) -> Option<PathBuf> {
        let result = match self.pending.as_ref()?.try_recv() {
            Ok(picked) => picked,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => None,
        };
        self.pending = None;
        result
    }
}

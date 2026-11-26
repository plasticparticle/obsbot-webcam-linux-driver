//! Talking to a running OBSBOT Control Center.
//!
//! The control center listens on a Unix socket in the user's runtime
//! directory. A client connects, writes one command line and reads one reply
//! line. `obsbotctl app …` uses this, which makes every command bindable to a
//! key in the desktop's own shortcut settings.

use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

/// Where the control center listens.
pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir);
    dir.join("obsbot-control-center.sock")
}

/// Send one command line and return the reply line.
///
/// Fails with `NotFound` or `ConnectionRefused` when no control center is
/// running.
pub fn send(command: &str) -> io::Result<String> {
    let mut stream = UnixStream::connect(socket_path())?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(command.replace(['\n', '\r'], " ").as_bytes())?;
    stream.write_all(b"\n")?;
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply)?;
    Ok(reply.trim_end().to_owned())
}

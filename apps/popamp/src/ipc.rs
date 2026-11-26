//! Single-instance control socket: a Unix socket, or a named pipe on
//! Windows.
//!
//! Protocol: the client sends one line per remote command (see
//! [`encode`]) and a final empty line; the server answers with one line
//! (status JSON or `ok`) and closes.

use crate::cli::Remote;
use popamp_core::Command;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Where the instance of `class` listens.
pub fn socket_path(class: &str) -> PathBuf {
    transport::path(&safe(class))
}

/// `s` with anything but letters, digits, `-` and `_` replaced.
fn safe(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn user() -> String {
    std::env::var("USER").or_else(|_| std::env::var("USERNAME")).unwrap_or_else(|_| "user".into())
}

#[cfg(unix)]
mod transport {
    use super::*;
    use std::os::unix::net::{UnixListener, UnixStream};

    pub fn path(class: &str) -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join(format!("popamp-{}", safe(&user()))));
        // Each Flatpak instance has a runtime dir of its own; only this one
        // is shared by all of them.
        let dir = match std::env::var_os("FLATPAK_ID") {
            Some(id) => dir.join("app").join(id),
            None => dir,
        };
        dir.join("popamp").join(format!("{class}.sock"))
    }

    pub fn connect(path: &Path) -> std::io::Result<UnixStream> {
        let s = UnixStream::connect(path)?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        Ok(s)
    }

    /// Binds the socket (replacing a stale one); yields each client.
    pub fn listen(path: &Path) -> std::io::Result<impl Iterator<Item = UnixStream> + Send + 'static> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        if path.exists() {
            // Only reached when nothing answered on it: stale.
            let _ = std::fs::remove_file(path);
        }
        let listener = UnixListener::bind(path)?;
        Ok(std::iter::from_fn(move || loop {
            if let Ok((s, _)) = listener.accept() {
                let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
                return Some(s);
            }
        }))
    }

    /// Nothing to wait for: the reply stays readable after a close.
    pub fn finish(_: &UnixStream) {}
}

#[cfg(windows)]
mod transport {
    use super::*;
    use std::fs::File;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Win32::Foundation::{ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
        PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };

    /// Pipe names are shared by all users of the machine, so the name has
    /// the user's in it.
    pub fn path(class: &str) -> PathBuf {
        PathBuf::from(format!(r"\\.\pipe\popamp-{}-{class}", safe(&user())))
    }

    /// Opens the pipe; while the server is busy with another client,
    /// waits up to 5 seconds for it.
    pub fn connect(path: &Path) -> std::io::Result<File> {
        let open = || std::fs::OpenOptions::new().read(true).write(true).open(path);
        match open() {
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
                // SAFETY: `name` is NUL-terminated.
                if unsafe { WaitNamedPipeW(name.as_ptr(), 5000) } == 0 {
                    return Err(e);
                }
                open()
            }
            other => other,
        }
    }

    /// One end of the pipe for the next client. The first one fails if
    /// another instance has the name.
    fn create(name: &[u16], first: bool) -> std::io::Result<File> {
        let flags = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
        // SAFETY: `name` is NUL-terminated; the handle, if valid, is ours.
        unsafe {
            let h = CreateNamedPipeW(name.as_ptr(), flags, mode, PIPE_UNLIMITED_INSTANCES, 4096, 4096, 0, std::ptr::null());
            if h == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error());
            }
            Ok(File::from_raw_handle(h))
        }
    }

    /// Takes the pipe's name; yields each client.
    pub fn listen(path: &Path) -> std::io::Result<impl Iterator<Item = File> + Send + 'static> {
        let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut next = Some(create(&name, true)?);
        Ok(std::iter::from_fn(move || loop {
            let pipe = match next.take().map_or_else(|| create(&name, false), Ok) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("control pipe: {e}");
                    return None;
                }
            };
            // SAFETY: a valid pipe handle; no overlapped I/O.
            let ok = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) } != 0;
            // A client that came between create and connect is connected too.
            if ok || std::io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                return Some(pipe);
            }
        }))
    }

    /// Waits until the client has read the reply: closing the server's end
    /// first would throw it away.
    pub fn finish(pipe: &File) {
        let _ = pipe.sync_all();
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn encode(r: &Remote) -> String {
    match r {
        Remote::Open(files) => format!("open\t{}", files.iter().map(|f| escape(f)).collect::<Vec<_>>().join("\t")),
        Remote::Add(files) => format!("add\t{}", files.iter().map(|f| escape(f)).collect::<Vec<_>>().join("\t")),
        Remote::Cmd(c) => format!("cmd\t{}", c.id()),
        Remote::SetVolumePercent(v) => format!("vol\t{v}"),
        Remote::VolumeStep(v) => format!("volstep\t{v}"),
        Remote::SetPan(v) => format!("pan\t{v}"),
        Remote::PanStep(v) => format!("panstep\t{v}"),
        Remote::Shuffle(v) => format!("shuffle\t{}", v.map(|b| (b as u8).to_string()).unwrap_or_default()),
        Remote::Repeat(v) => format!("repeat\t{}", v.map(|b| (b as u8).to_string()).unwrap_or_default()),
        Remote::SeekMs(ms) => format!("seek\t{ms}"),
        Remote::Clear => "clear".into(),
        Remote::Show => "show".into(),
        Remote::Quit => "quit".into(),
        Remote::Status => "status".into(),
    }
}

pub fn decode(line: &str) -> Option<Remote> {
    let mut parts = line.split('\t');
    let verb = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    let arg = rest.first().copied().unwrap_or("");
    let opt_bool = |s: &str| match s {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    };
    Some(match verb {
        "open" => Remote::Open(rest.iter().filter(|s| !s.is_empty()).map(|s| unescape(s)).collect()),
        "add" => Remote::Add(rest.iter().filter(|s| !s.is_empty()).map(|s| unescape(s)).collect()),
        "cmd" => Remote::Cmd(Command::from_id(arg.parse().ok()?)?),
        "vol" => Remote::SetVolumePercent(arg.parse().ok()?),
        "volstep" => Remote::VolumeStep(arg.parse().ok()?),
        "pan" => Remote::SetPan(arg.parse().ok()?),
        "panstep" => Remote::PanStep(arg.parse().ok()?),
        "shuffle" => Remote::Shuffle(opt_bool(arg)),
        "repeat" => Remote::Repeat(opt_bool(arg)),
        "seek" => Remote::SeekMs(arg.parse().ok()?),
        "clear" => Remote::Clear,
        "show" => Remote::Show,
        "quit" => Remote::Quit,
        "status" => Remote::Status,
        _ => return None,
    })
}

/// Sends commands to a running instance. Returns its reply, or `Err` if
/// none is running.
pub fn send(path: &Path, remotes: &[Remote]) -> std::io::Result<String> {
    let s = transport::connect(path)?;
    let mut msg = String::new();
    for r in remotes {
        msg.push_str(&encode(r));
        msg.push('\n');
    }
    msg.push('\n');
    (&s).write_all(msg.as_bytes())?;
    let mut reply = String::new();
    BufReader::new(&s).read_line(&mut reply)?;
    Ok(reply.trim_end().to_string())
}

#[derive(Debug)]
pub struct Request {
    pub remotes: Vec<Remote>,
    pub reply: crossbeam_channel::Sender<String>,
}

/// Listens on `path` and serves on a thread.
pub fn serve(path: PathBuf, deliver: impl Fn(Request) + Send + 'static) -> std::io::Result<()> {
    let clients = transport::listen(&path)?;
    std::thread::Builder::new().name("popamp-ipc".into()).spawn(move || {
        for client in clients {
            answer(&client, &deliver);
            transport::finish(&client);
        }
    })?;
    Ok(())
}

/// Reads one client's commands, hands them over and writes the reply.
/// `client` is a shared reference to the connection, which reads and
/// writes as itself. (A bound on all references, `for<'a> &'a S`, would
/// send the compiler round objc2's impls for ever on macOS.)
fn answer<S: Read + Write + Copy>(client: S, deliver: &impl Fn(Request)) {
    let mut reader = BufReader::new(client);
    let mut remotes = Vec::new();
    let mut line = String::new();
    while reader.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
        let l = line.trim_end_matches('\n');
        if l.is_empty() {
            break;
        }
        if let Some(r) = decode(l) {
            remotes.push(r);
        }
        line.clear();
    }
    let (tx, rx) = crossbeam_channel::bounded(1);
    deliver(Request { remotes, reply: tx });
    let reply = rx.recv_timeout(Duration::from_secs(4)).unwrap_or_else(|_| "ok".into());
    let mut out = client;
    let _ = out.write_all(format!("{reply}\n").as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn socket_paths_fit_a_macos_socket_address() {
        for class in ["default".to_string(), format!("new-{}", u32::MAX)] {
            let path = socket_path(&class);
            assert!(path.as_os_str().len() < 104, "{}", path.display());
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        let all = [
            Remote::Open(vec!["/a b/c\td.mp3".into(), "line\nbreak".into()]),
            Remote::Add(vec!["x".into()]),
            Remote::Cmd(Command::Next),
            Remote::SetVolumePercent(40),
            Remote::VolumeStep(-5),
            Remote::SetPan(-30),
            Remote::PanStep(6),
            Remote::Shuffle(None),
            Remote::Repeat(Some(true)),
            Remote::SeekMs(1234),
            Remote::Clear,
            Remote::Show,
            Remote::Quit,
            Remote::Status,
        ];
        for r in all {
            if let Remote::Open(files) = &r {
                // Tabs inside names are not representable; skip that case.
                if files.iter().any(|f| f.contains('\t')) {
                    continue;
                }
            }
            assert_eq!(decode(&encode(&r)), Some(r.clone()), "{r:?}");
        }
        assert_eq!(decode(&encode(&Remote::Open(vec!["line\nbreak".into()]))), Some(Remote::Open(vec!["line\nbreak".into()])));
    }

    #[test]
    fn socket_round_trip() {
        let path = socket_path(&format!("test-{}", std::process::id()));
        serve(path.clone(), |req| {
            let _ = req.reply.send(format!("got {}", req.remotes.len()));
        })
        .unwrap();
        let reply = send(&path, &[Remote::Status, Remote::Cmd(Command::Play)]).unwrap();
        assert_eq!(reply, "got 2");
        let _ = std::fs::remove_file(&path);
    }
}

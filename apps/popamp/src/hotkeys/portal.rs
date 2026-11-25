//! Global hotkeys under Wayland, through the desktop portal: the desktop
//! asks the user once which keys to give, and signals each press. KDE
//! Plasma 6, GNOME 48 and Hyprland serve it.

use super::{Key, Send, BINDINGS};
use std::collections::HashMap;
use std::thread::JoinHandle;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::MatchRule;

pub const PORTAL: &str = "org.freedesktop.portal.Desktop";
pub const PATH: &str = "/org/freedesktop/portal/desktop";
pub const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

impl Key {
    /// The name the desktop keeps the shortcut by.
    pub(super) fn id(self) -> &'static str {
        match self {
            Key::Insert => "play",
            Key::Home => "pause",
            Key::End => "stop",
            Key::PageUp => "previous",
            Key::PageDown => "next",
            Key::Up => "volume-up",
            Key::Down => "volume-down",
            Key::Left => "back",
            Key::Right => "forward",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Key::Insert => tr!("hotkeys.play"),
            Key::Home => tr!("hotkeys.pause"),
            Key::End => tr!("hotkeys.stop"),
            Key::PageUp => tr!("hotkeys.previousTrack"),
            Key::PageDown => tr!("hotkeys.nextTrack"),
            Key::Up => tr!("hotkeys.volumeUp"),
            Key::Down => tr!("hotkeys.volumeDown"),
            Key::Left => tr!("hotkeys.back5Seconds"),
            Key::Right => tr!("hotkeys.forward5Seconds"),
        }
    }

    /// The keys asked for, which the user may change.
    pub(super) fn trigger(self) -> &'static str {
        match self {
            Key::Insert => "CTRL+ALT+Insert",
            Key::Home => "CTRL+ALT+Home",
            Key::End => "CTRL+ALT+End",
            Key::PageUp => "CTRL+ALT+Page_Up",
            Key::PageDown => "CTRL+ALT+Page_Down",
            Key::Up => "CTRL+ALT+Up",
            Key::Down => "CTRL+ALT+Down",
            Key::Left => "CTRL+ALT+Left",
            Key::Right => "CTRL+ALT+Right",
        }
    }
}

/// The shortcuts bound for this run; they go with the session.
pub struct Session {
    conn: Option<Connection>,
    portal: String,
    handle: OwnedObjectPath,
    thread: Option<JoinHandle<()>>,
}

/// Binds the keys through the desktop's portal.
pub fn bind(send: Send) -> Option<Session> {
    bind_at(PORTAL, send).map_err(|e| log::info!("no global hotkeys: {e}")).ok()
}

/// Binds the keys through the portal named `portal`.
pub(super) fn bind_at(portal: &str, send: Send) -> zbus::Result<Session> {
    let conn = Connection::session()?;
    if portal == PORTAL {
        // Newer portals know a program outside a sandbox only by this, and
        // only if it comes first; older ones lack it and do without.
        let _ = conn.call_method(Some(portal), PATH, Some("org.freedesktop.host.portal.Registry"), "Register", &("popamp", options([])));
    }
    let token = format!("popamp{}", std::process::id());
    let created = request(&conn, &format!("{token}s"), |path_token| {
        let opts = options([("handle_token", path_token.into()), ("session_handle_token", token.as_str().into())]);
        conn.call_method(Some(portal), PATH, Some(SHORTCUTS), "CreateSession", &(opts,)).map(drop)
    })?;
    let handle = created.get("session_handle").and_then(|v| String::try_from(v.clone()).ok()).ok_or_else(|| failure("no session"))?;
    let handle = OwnedObjectPath::try_from(handle)?;
    let shortcuts: Vec<(&str, HashMap<&str, Value>)> = BINDINGS
        .iter()
        .map(|&(key, _)| (key.id(), options([("description", key.description().into()), ("preferred_trigger", key.trigger().into())])))
        .collect();
    // Subscribed before binding: the first press may come right after.
    let rule = MatchRule::builder().msg_type(Type::Signal).interface(SHORTCUTS)?.member("Activated")?.path(PATH)?.build();
    let presses = MessageIterator::for_match_rule(rule, &conn, None)?;
    request(&conn, &format!("{token}b"), |path_token| {
        let opts = options([("handle_token", path_token.into())]);
        conn.call_method(Some(portal), PATH, Some(SHORTCUTS), "BindShortcuts", &(&handle, shortcuts, "", opts)).map(drop)
    })?;
    let session = handle.clone();
    let thread = std::thread::Builder::new()
        .name("popamp-hotkeys".into())
        .spawn(move || listen(presses, &session, &send))
        .map_err(|e| failure(&e.to_string()))?;
    Ok(Session { conn: Some(conn), portal: portal.into(), handle, thread: Some(thread) })
}

fn options<'a, const N: usize>(pairs: [(&'a str, Value<'a>); N]) -> HashMap<&'a str, Value<'a>> {
    HashMap::from(pairs)
}

fn failure(why: &str) -> zbus::Error {
    zbus::Error::Failure(why.into())
}

/// Makes a call that the portal answers later, with a Response signal on
/// a request object named after `token`, and waits for the answer.
fn request(conn: &Connection, token: &str, call: impl FnOnce(&str) -> zbus::Result<()>) -> zbus::Result<HashMap<String, OwnedValue>> {
    let sender = conn.unique_name().ok_or_else(|| failure("no bus name"))?.trim_start_matches(':').replace('.', "_");
    let path = format!("{PATH}/request/{sender}/{token}");
    let rule =
        MatchRule::builder().msg_type(Type::Signal).interface("org.freedesktop.portal.Request")?.member("Response")?.path(path)?.build();
    // Subscribed before the call, which the answer may overtake.
    let mut answers = MessageIterator::for_match_rule(rule, conn, Some(1))?;
    call(token)?;
    let answer = answers.next().ok_or_else(|| failure("no answer"))??;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize()?;
    match code {
        0 => Ok(results),
        1 => Err(failure("the user said no")),
        _ => Err(failure("the desktop said no")),
    }
}

/// Sends the command of each key pressed, until the connection closes.
fn listen(presses: MessageIterator, session: &ObjectPath<'_>, send: &Send) {
    for press in presses {
        let Ok(press) = press else { return };
        let body = press.body();
        let Ok((of, id, ..)) = body.deserialize::<(ObjectPath<'_>, String, u64, HashMap<String, OwnedValue>)>() else { continue };
        if of == *session {
            if let Some(&(_, command)) = BINDINGS.iter().find(|(key, _)| key.id() == id) {
                send(command);
            }
        }
    }
}

impl Drop for Session {
    /// Ends the session, which takes the keys back, and the listener.
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            let _ = conn.call_method(Some(self.portal.as_str()), &self.handle, Some("org.freedesktop.portal.Session"), "Close", &());
            let _ = conn.close();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use popamp_core::Command;
    use std::sync::Arc;
    use zbus::message::Header;
    use zbus::object_server::SignalEmitter;

    /// A portal that grants whatever is asked, and keeps what was bound.
    struct FakePortal {
        bound: Arc<Mutex<Vec<(String, String)>>>,
    }

    impl FakePortal {
        async fn answer(conn: &zbus::Connection, header: &Header<'_>, token: &str, results: HashMap<&str, Value<'_>>) -> OwnedObjectPath {
            let sender = header.sender().unwrap().to_string();
            let path = format!("{PATH}/request/{}/{token}", sender.trim_start_matches(':').replace('.', "_"));
            conn.emit_signal(Some(sender.as_str()), path.as_str(), "org.freedesktop.portal.Request", "Response", &(0u32, results))
                .await
                .unwrap();
            OwnedObjectPath::try_from(path).unwrap()
        }
    }

    #[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
    impl FakePortal {
        async fn create_session(
            &self,
            options: HashMap<String, OwnedValue>,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &zbus::Connection,
        ) -> OwnedObjectPath {
            let token = String::try_from(options["handle_token"].clone()).unwrap();
            let session = format!("{PATH}/session/x/{}", String::try_from(options["session_handle_token"].clone()).unwrap());
            Self::answer(conn, &header, &token, HashMap::from([("session_handle", Value::from(session))])).await
        }

        async fn bind_shortcuts(
            &self,
            _session: ObjectPath<'_>,
            shortcuts: Vec<(String, HashMap<String, OwnedValue>)>,
            _parent: String,
            options: HashMap<String, OwnedValue>,
            #[zbus(header)] header: Header<'_>,
            #[zbus(connection)] conn: &zbus::Connection,
        ) -> OwnedObjectPath {
            let trigger = |o: &HashMap<String, OwnedValue>| String::try_from(o["preferred_trigger"].clone()).unwrap();
            *self.bound.lock() = shortcuts.iter().map(|(id, o)| (id.clone(), trigger(o))).collect();
            let token = String::try_from(options["handle_token"].clone()).unwrap();
            Self::answer(conn, &header, &token, HashMap::new()).await
        }

        #[zbus(signal)]
        async fn activated(
            e: &SignalEmitter<'_>,
            session: ObjectPath<'_>,
            id: &str,
            at: u64,
            options: HashMap<&str, Value<'_>>,
        ) -> zbus::Result<()>;
    }

    struct FakeSession(Arc<Mutex<bool>>);

    #[zbus::interface(name = "org.freedesktop.portal.Session")]
    impl FakeSession {
        fn close(&self) {
            *self.0.lock() = true;
        }
    }

    #[test]
    fn keys_are_asked_for_and_their_presses_arrive() {
        let name = format!("org.popamp.FakePortal{}", std::process::id());
        let (bound, closed) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(false)));
        let portal = FakePortal { bound: bound.clone() };
        let served =
            zbus::blocking::connection::Builder::session().and_then(|b| b.name(name.as_str())).and_then(|b| b.serve_at(PATH, portal));
        let Ok(fake) = served.and_then(|b| b.build()) else {
            eprintln!("no session bus: skipped");
            return;
        };
        let session = format!("{PATH}/session/x/popamp{}", std::process::id());
        fake.object_server().at(session.as_str(), FakeSession(closed.clone())).unwrap();

        let (tx, rx) = crossbeam_channel::unbounded();
        let keys = bind_at(&name, Arc::new(move |c| tx.send(c).unwrap())).unwrap();
        assert_eq!(bound.lock().len(), BINDINGS.len());
        assert!(bound.lock().contains(&("next".into(), "CTRL+ALT+Page_Down".into())), "{:?}", bound.lock());

        let press = |of: &str, id: &str| {
            let at = ObjectPath::try_from(of).unwrap();
            fake.emit_signal(None::<&str>, PATH, SHORTCUTS, "Activated", &(at, id, 0u64, options([]))).unwrap();
        };
        press("/org/freedesktop/portal/desktop/session/x/other", "stop");
        press(&session, "next");
        press(&session, "volume-up");
        let timeout = std::time::Duration::from_secs(5);
        assert_eq!([rx.recv_timeout(timeout).unwrap(), rx.recv_timeout(timeout).unwrap()], [Command::Next, Command::VolumeUp]);

        let (done, ended) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            drop(keys);
            let _ = done.send(());
        });
        assert!(ended.recv_timeout(timeout).is_ok(), "letting go returns");
        assert!(*closed.lock(), "the session is closed");
        assert!(rx.try_recv().is_err(), "another session's press is not ours");
    }
}

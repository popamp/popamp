//! Track notifications through the freedesktop notification server, which
//! every desktop runs. Each one replaces the last, so a skipped-through
//! album leaves one bubble, not a stack.

use super::{byline, Track};
use std::collections::HashMap;
use std::path::Path;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

pub struct Notifier {
    conn: Connection,
    /// The bubble on show, for the next one to replace.
    shown: u32,
    /// Whether the server reads markup in the body, so `&` must be escaped.
    markup: Option<bool>,
}

impl Notifier {
    pub fn new(conn: Connection) -> Notifier {
        Notifier { conn, shown: 0, markup: None }
    }

    /// Shows `track`, with its cover when it has one.
    pub fn show(&mut self, track: &Track, cover: Option<&Path>) {
        if let Err(e) = self.notify(track, cover) {
            log::debug!("notification: {e}");
        }
    }

    fn notify(&mut self, track: &Track, cover: Option<&Path>) -> zbus::Result<()> {
        let markup = *self.markup.get_or_insert_with(|| capabilities(&self.conn).iter().any(|c| c == "body-markup"));
        let body = if markup { escape(&byline(track)) } else { byline(track) };
        let icon = cover.map_or_else(|| crate::flatpak::desktop_id().to_string(), |p| popamp_core::text::path_to_file_url(&p.to_string_lossy()));
        // Transient: a track that played is no message to keep.
        let hints: HashMap<&str, Value> = HashMap::from([("desktop-entry", Value::from(crate::flatpak::desktop_id())), ("transient", Value::from(true))]);
        let args = (crate::edition::NAME, self.shown, icon.as_str(), track.title.as_str(), body.as_str(), Vec::<&str>::new(), hints, -1i32);
        self.shown = self.conn.call_method(Some(NAME), PATH, Some(NAME), "Notify", &args)?.body().deserialize()?;
        Ok(())
    }
}

/// What the server can do; none when it does not say.
fn capabilities(conn: &Connection) -> Vec<String> {
    let reply = conn.call_method(Some(NAME), PATH, Some(NAME), "GetCapabilities", &());
    reply.and_then(|r| r.body().deserialize()).unwrap_or_default()
}

/// The body markup is a little HTML.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_escaped() {
        assert_eq!(escape("Simon & Garfunkel <live>"), "Simon &amp; Garfunkel &lt;live&gt;");
    }
}

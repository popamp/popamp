//! Text helpers shared by the parsers.

use encoding_rs::Encoding;

/// Decodes UTF-8 (with or without BOM); otherwise guesses the legacy code
/// page (Windows-1251/1252, EUC-KR, GBK, Shift_JIS, Big5, ...) from the text.
pub fn decode_lossy(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    legacy_code_page([bytes], region()).decode_without_bom_handling(bytes).0.into_owned()
}

/// Where old text most likely comes from, as a country's internet domain
/// ("kr", "cn", "ru"), or empty when unknown: a few bytes of a short title
/// fit more than one code page, and the user's language tells which.
static REGION: std::sync::RwLock<&'static str> = std::sync::RwLock::new("");

/// Says where old text most likely comes from; see [`REGION`].
pub fn set_region(domain: &'static str) {
    *REGION.write().unwrap_or_else(|e| e.into_inner()) = domain;
}

fn region() -> &'static str {
    *REGION.read().unwrap_or_else(|e| e.into_inner())
}

/// The code page that reads `samples` best, judged on all of them together.
fn legacy_code_page<'a>(samples: impl IntoIterator<Item = &'a [u8]>, region: &str) -> &'static Encoding {
    let mut detector = chardetng::EncodingDetector::new();
    let mut cyrillic = false;
    for sample in samples {
        detector.feed(sample, false);
        detector.feed(b"\n", false);
        cyrillic |= looks_cyrillic(sample);
    }
    detector.feed(b"", true);
    match detector.guess(Some(region.as_bytes()).filter(|r| !r.is_empty()), false) {
        // The detector takes a lone short Russian word ("Тучи") for Western.
        guess if guess == encoding_rs::WINDOWS_1252 && cyrillic => encoding_rs::WINDOWS_1251,
        guess => guess,
    }
}

/// Cyrillic words in Windows-1251 are runs of bytes 0xC0..=0xFF (А..я);
/// Western text has an accented letter here and there, seldom three in a row.
fn looks_cyrillic(bytes: &[u8]) -> bool {
    bytes.split(|&b| b < 0xC0).any(|run| run.len() >= 3)
}

/// Re-reads tag text that a tag reader decoded one char per byte (ISO-8859-1,
/// as ID3v1 and old ID3v2 frames say), though the tagger wrote its system code
/// page or UTF-8. One file's values share a code page, so they are judged together.
pub fn redecode_latin1<'a>(values: impl IntoIterator<Item = &'a mut String>) {
    redecode_latin1_from(region(), values);
}

fn redecode_latin1_from<'a>(region: &str, values: impl IntoIterator<Item = &'a mut String>) {
    let as_bytes = |s: &str| s.chars().map(|c| u8::try_from(c).ok()).collect::<Option<Vec<u8>>>();
    let legacy: Vec<(&mut String, Vec<u8>)> =
        values.into_iter().filter_map(|v| as_bytes(v).filter(|b| !b.is_ascii()).map(|b| (v, b))).collect();
    if legacy.is_empty() {
        return;
    }
    let code_page = legacy_code_page(legacy.iter().filter(|(_, b)| std::str::from_utf8(b).is_err()).map(|(_, b)| b.as_slice()), region);
    for (value, bytes) in legacy {
        *value = match String::from_utf8(bytes) {
            Ok(utf8) => utf8,
            Err(e) => code_page.decode_without_bom_handling(e.as_bytes()).0.into_owned(),
        };
    }
}

/// Decodes `%XX` escapes (as found in `file://` URLs).
pub fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    decode_lossy(&out)
}

/// Encodes a filesystem path as a `file://` URL.
pub fn path_to_file_url(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else { break };
        let ent = &rest[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Formats milliseconds as `m:ss` or `h:mm:ss`.
pub fn format_duration_ms(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Compares strings so that embedded numbers sort numerically ("2" < "10").
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != Ordering::Equal {
                    return ord;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Keeps a field of a tab-separated line on its line: backslash, tab and line breaks are escaped.
pub fn escape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

/// Undoes [`escape_field`].
pub fn unescape_field(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_cp1252() {
        assert_eq!(decode_lossy(b"caf\xe9 \x93x\x94"), "café “x”");
        assert_eq!(decode_lossy("naïve".as_bytes()), "naïve");
    }

    #[test]
    fn decodes_cyrillic_cp1251() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode("TITLE \"Тучи\" — Иванушки");
        assert_eq!(decode_lossy(&bytes), "TITLE \"Тучи\" — Иванушки");
        assert_eq!(decode_lossy(b"Gr\xf6\xdfe \xc0 la"), "Größe À la", "short runs stay Western");
    }

    #[test]
    fn decodes_east_asian_code_pages() {
        for (enc, text) in [
            (encoding_rs::EUC_KR, "TITLE \"난 알아요\"\nPERFORMER \"서태지와 아이들\""),
            (encoding_rs::GBK, "TITLE \"晴天\"\nPERFORMER \"周杰伦\""),
            (encoding_rs::SHIFT_JIS, "TITLE \"ファースト・ラブ\"\nPERFORMER \"宇多田ヒカル\""),
            (encoding_rs::BIG5, "TITLE \"月亮代表我的心\"\nPERFORMER \"鄧麗君\""),
        ] {
            assert_eq!(decode_lossy(&enc.encode(text).0), text, "{}", enc.name());
        }
    }

    /// Tag readers hand over old tag text one char per byte; the bytes were the tagger's code page.
    fn as_latin1(enc: &'static encoding_rs::Encoding, text: &str) -> String {
        enc.encode(text).0.iter().map(|&b| b as char).collect()
    }

    #[test]
    fn redecodes_tag_text_read_as_latin1() {
        for (enc, fields) in [
            (encoding_rs::EUC_KR, ["난 알아요", "서태지와 아이들", "Seotaiji and Boys"]),
            (encoding_rs::GBK, ["晴天", "周杰伦", "叶惠美"]),
            (encoding_rs::WINDOWS_1251, ["Тучи", "Иванушки International", "Конфетки-бараночки"]),
        ] {
            let mut values: Vec<String> = fields.iter().map(|t| as_latin1(enc, t)).collect();
            redecode_latin1(values.iter_mut());
            assert_eq!(values, fields, "{}", enc.name());
        }
    }

    #[test]
    fn a_lone_short_title_is_read_in_the_code_page_of_the_users_region() {
        for (region, enc, title) in
            [("kr", encoding_rs::EUC_KR, "난 알아요"), ("cn", encoding_rs::GBK, "晴天"), ("ru", encoding_rs::WINDOWS_1251, "Тучи")]
        {
            let mut values = [as_latin1(enc, title)];
            redecode_latin1_from(region, values.iter_mut());
            assert_eq!(values, [title], "{}", enc.name());
        }
    }

    #[test]
    fn leaves_real_latin1_and_unicode_tag_text_alone() {
        let mut values = ["Björk", "Sigur Rós", "Café del Mar", "서태지", "plain"].map(String::from);
        redecode_latin1(values.iter_mut());
        assert_eq!(values, ["Björk", "Sigur Rós", "Café del Mar", "서태지", "plain"]);
    }

    #[test]
    fn utf8_written_into_a_latin1_frame_reads_as_utf8() {
        let mut values = [as_latin1(encoding_rs::UTF_8, "서태지"), as_latin1(encoding_rs::UTF_8, "Тучи")];
        redecode_latin1(values.iter_mut());
        assert_eq!(values, ["서태지", "Тучи"]);
    }

    #[test]
    fn percent_round_trip() {
        let p = "/music/A & B/01 – song.mp3";
        let url = path_to_file_url(p);
        assert_eq!(percent_decode(url.trim_start_matches("file://")), p);
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn xml_entities() {
        assert_eq!(xml_unescape("a &amp; b &#65;&#x42; &bogus"), "a & b AB &bogus");
        assert_eq!(xml_unescape(&xml_escape("<a & \"b\">")), "<a & \"b\">");
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["track10", "Track2", "track1"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, ["track1", "Track2", "track10"]);
    }

    #[test]
    fn durations() {
        assert_eq!(format_duration_ms(61_000), "1:01");
        assert_eq!(format_duration_ms(3_725_000), "1:02:05");
    }
}

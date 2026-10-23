//! Removes secrets and personal path segments from log output.
//!
//! Applied to every formatted event before it reaches the log file or stderr, so panics,
//! error chains and span fields are covered without each call site redacting its own text.

use std::{
    io::{self, Write},
    sync::{OnceLock, RwLock},
};

const REDACTED: &str = "[redacted]";
const USER: &str = "<user>";
// Values following these markers are credentials, e.g. `Authorization: Bearer ...` or
// `?api_key=...`. `token=` also covers `access_token=`.
const MARKERS: &[&str] = &["bearer ", "token=", "api_key=", "oauth="];

static SECRETS: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Redacts these values, such as provider tokens, from all subsequent log output.
pub fn register_secrets<S: AsRef<str>>(values: impl IntoIterator<Item = S>) {
    let Ok(mut secrets) = SECRETS.write() else {
        return;
    };
    for value in values {
        let value = value.as_ref().trim();
        // Very short values would erase unrelated text; real credentials are long.
        if value.len() >= 4 && !secrets.iter().any(|s| s == value) {
            secrets.push(value.to_owned());
        }
    }
}

/// Redacts registered secrets, credential parameters and the current user's name in paths.
pub fn redact(text: &str) -> String {
    let secrets = SECRETS.read().map(|s| s.clone()).unwrap_or_default();
    redact_with(text, &secrets, user_names())
}

fn user_names() -> &'static [String] {
    static NAMES: OnceLock<Vec<String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        // Under Wine/Proton, the Linux account appears through USER/HOME and paths such as
        // `Z:\home\<name>`, while USERNAME is the prefix's own user.
        let mut names: Vec<String> = ["USERNAME", "USER", "LOGNAME"]
            .iter()
            .filter_map(std::env::var_os)
            .chain(std::env::var_os("HOME").and_then(|home| {
                std::path::Path::new(&home)
                    .file_name()
                    .map(ToOwned::to_owned)
            }))
            .chain(std::env::home_dir().and_then(|home| home.file_name().map(ToOwned::to_owned)))
            .filter_map(|name| name.into_string().ok())
            .filter(|name| name.len() >= 2)
            .collect();
        names.sort();
        names.dedup();
        names
    })
}

fn redact_with(text: &str, secrets: &[String], user_names: &[String]) -> String {
    let mut text = text.to_owned();
    for secret in secrets {
        text = text.replace(secret.as_str(), REDACTED);
    }
    for name in user_names {
        text = replace_path_segment(&text, name);
    }
    redact_markers(&text)
}

fn redact_markers(text: &str) -> String {
    // ASCII lowercasing keeps byte offsets identical to the original text.
    let lower = text.to_ascii_lowercase();
    let mut ranges = Vec::new();
    for marker in MARKERS {
        let mut from = 0;
        while let Some(found) = lower[from..].find(marker) {
            let start = from + found + marker.len();
            let end = text[start..]
                .find(|c: char| c.is_whitespace() || "&<>\"',;)".contains(c))
                .map_or(text.len(), |end| start + end);
            if end > start && !text[start..end].starts_with(REDACTED) {
                ranges.push(start..end);
            }
            from = start;
        }
    }
    ranges.sort_by_key(|range| range.start);

    let mut redacted = String::with_capacity(text.len());
    let mut copied = 0;
    for range in ranges {
        if range.start < copied {
            continue;
        }
        redacted.push_str(&text[copied..range.start]);
        redacted.push_str(REDACTED);
        copied = range.end;
    }
    redacted.push_str(&text[copied..]);
    redacted
}

/// Replaces `name` where it forms a whole path segment, such as `C:\Users\name\...`.
fn replace_path_segment(text: &str, name: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let name_lower = name.to_ascii_lowercase();
    let mut replaced = String::with_capacity(text.len());
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = lower[from..].find(&name_lower) {
        let start = from + found;
        let end = start + name.len();
        let after = text[end..].chars().next();
        let segment = text[..start].ends_with(['\\', '/'])
            && after.is_none_or(|c| matches!(c, '\\' | '/' | '"' | '\'') || c.is_whitespace());
        if segment {
            replaced.push_str(&text[copied..start]);
            replaced.push_str(USER);
            copied = end;
        }
        from = end;
    }
    replaced.push_str(&text[copied..]);
    replaced
}

/// Wraps a [`tracing_subscriber::fmt::MakeWriter`] so every formatted event is redacted.
pub(crate) struct Redacting<M>(pub(crate) M);

impl<'a, M: tracing_subscriber::fmt::MakeWriter<'a>> tracing_subscriber::fmt::MakeWriter<'a>
    for Redacting<M>
{
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter(self.0.make_writer())
    }
}

pub(crate) struct RedactingWriter<W>(W);

impl<W: Write> Write for RedactingWriter<W> {
    // The fmt layer writes each formatted event in a single call, so a value cannot be
    // split across two writes.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .write_all(redact(&String::from_utf8_lossy(buf)).as_bytes())?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn redact(text: &str) -> String {
        redact_with(
            text,
            &["s3cr3t-token-value".into()],
            &["Alice".into(), "steamuser".into()],
        )
    }

    #[test]
    fn registered_secrets_are_removed_everywhere() {
        assert_eq!(
            redact("error: invalid s3cr3t-token-value (s3cr3t-token-value)"),
            "error: invalid [redacted] ([redacted])"
        );
    }

    #[test]
    fn credential_parameters_are_removed() {
        assert_eq!(
            redact("GET /v1/games?api_key=abc123&id=5 Authorization: Bearer eyJ.x.y"),
            "GET /v1/games?api_key=[redacted]&id=5 Authorization: Bearer [redacted]"
        );
        assert_eq!(
            redact("https://host/f?ACCESS_TOKEN=xyz oauth=q"),
            "https://host/f?ACCESS_TOKEN=[redacted] oauth=[redacted]"
        );
        assert_eq!(redact("token=[redacted]"), "token=[redacted]");
        assert_eq!(redact("missing OAuth token"), "missing OAuth token");
    }

    #[test]
    fn user_names_are_removed_only_as_path_segments() {
        assert_eq!(
            redact(r"config dir = C:\Users\alice\AppData\Roaming\mint"),
            r"config dir = C:\Users\<user>\AppData\Roaming\mint"
        );
        assert_eq!(
            redact(r#"open "C:\\Users\\Alice\\mods" Z:\home\Alice"#),
            r#"open "C:\\Users\\<user>\\mods" Z:\home\<user>"#
        );
        assert_eq!(
            redact("/home/Alice/.steam and C:/users/steamuser/x"),
            "/home/<user>/.steam and C:/users/<user>/x"
        );
        assert_eq!(
            redact("https://github.com/alicedev/mint by Alice"),
            "https://github.com/alicedev/mint by Alice"
        );
    }

    #[test]
    fn short_values_are_not_registered() {
        register_secrets(["", "ab", "  "]);
        assert!(SECRETS.read().unwrap().iter().all(|s| s.len() >= 4));
    }
}

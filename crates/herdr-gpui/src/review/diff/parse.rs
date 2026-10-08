//! `git diff` output, made with `a/` and `b/` prefixes, as files and their
//! numbered lines.
use super::{Body, Kind, Lines, Status, push_clean};
use std::sync::Arc;

/// One file as the diff gave it.
#[derive(Debug)]
pub(super) struct Parsed {
    pub path: String,
    pub old_path: Option<String>,
    pub status: Status,
    pub body: Body,
}

/// A path from a `---`/`+++` line or the `diff --git` header, without its
/// `a/`/`b/` prefix or Git's quoting.
pub(super) fn path(text: &str, prefix: &str) -> String {
    let text = text.trim_end_matches('\r');
    let text = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .unwrap_or(text);
    let mut clean = String::new();
    push_clean(&mut clean, text.strip_prefix(prefix).unwrap_or(text));
    clean
}

/// The starts of `@@ -a,b +c,d @@`: before and after the change.
pub(crate) fn hunk_starts(header: &str) -> Option<(u32, u32)> {
    let mut parts = header.strip_prefix("@@ ")?.split(' ');
    let start = |part: Option<&str>, sign: char| -> Option<u32> {
        part?.strip_prefix(sign)?.split(',').next()?.parse().ok()
    };
    Some((start(parts.next(), '-')?, start(parts.next(), '+')?))
}

/// The file being read, until the next header.
struct Open {
    parsed: Parsed,
    lines: Lines,
    binary: bool,
    /// Where the next lines are numbered from, inside a hunk.
    hunk: Option<(u32, u32)>,
}

impl Open {
    fn close(self) -> Parsed {
        let mut parsed = self.parsed;
        parsed.body = if self.binary {
            Body::Binary
        } else {
            Body::Loaded(Arc::new(self.lines.finish()))
        };
        parsed
    }

    fn line(&mut self, raw: &str) {
        if raw.starts_with("@@ ") {
            self.hunk = hunk_starts(raw);
            self.lines.push(Kind::Hunk, None, None, raw);
            return;
        }
        let Some((old, new)) = self.hunk.as_mut() else {
            self.header(raw);
            return;
        };
        let (kind, text) = match raw.chars().next() {
            Some(' ') => (Kind::Context, &raw[1..]),
            Some('+') => (Kind::Added, &raw[1..]),
            Some('-') => (Kind::Removed, &raw[1..]),
            Some('\\') => (Kind::Meta, raw),
            _ => return,
        };
        let numbers = match kind {
            Kind::Context => (Some(*old), Some(*new)),
            Kind::Added => (None, Some(*new)),
            Kind::Removed => (Some(*old), None),
            _ => (None, None),
        };
        if self.lines.push(kind, numbers.0, numbers.1, text) {
            if numbers.0.is_some() {
                *old = old.saturating_add(1);
            }
            if numbers.1.is_some() {
                *new = new.saturating_add(1);
            }
        }
    }

    /// A line between the `diff --git` header and the first hunk.
    fn header(&mut self, raw: &str) {
        let parsed = &mut self.parsed;
        if let Some(name) = raw.strip_prefix("+++ ") {
            if name != "/dev/null" {
                parsed.path = path(name, "b/");
            }
        } else if raw.starts_with("new file mode") || raw.starts_with("copy from") {
            parsed.status = Status::Added;
        } else if raw.starts_with("deleted file mode") {
            parsed.status = Status::Deleted;
        } else if let Some(from) = raw.strip_prefix("rename from ") {
            parsed.status = Status::Renamed;
            parsed.old_path = Some(path(from, ""));
        } else if raw.starts_with("Binary files") || raw == "GIT binary patch" {
            self.binary = true;
        }
    }
}

/// The files of a diff, in its order.
pub(super) fn parse(text: &str) -> Vec<Parsed> {
    let mut files = Vec::new();
    let mut open: Option<Open> = None;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        // Inside a hunk every content line starts with a space, `+`, `-` or
        // `\`, so a header line can only start a new file.
        if let Some(header) = raw.strip_prefix("diff --git ") {
            files.extend(open.take().map(Open::close));
            let name = header
                .rfind(" b/")
                .map_or(header, |index| &header[index + 1..]);
            open = Some(Open {
                parsed: Parsed {
                    path: path(name, "b/"),
                    old_path: None,
                    status: Status::Modified,
                    body: Body::Pending,
                },
                lines: Lines::default(),
                binary: false,
                hunk: None,
            });
            continue;
        }
        if let Some(open) = open.as_mut() {
            open.line(raw);
        }
    }
    files.extend(open.map(Open::close));
    files
}

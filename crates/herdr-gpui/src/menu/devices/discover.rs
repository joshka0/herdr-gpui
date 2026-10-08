//! Suggestions for the Add Device form: SSH hosts advertised over Bonjour,
//! online Tailscale peers, and Host aliases in `~/.ssh/config`. Each search
//! runs every source once on the background executor for a bounded time, and
//! the open form drains what they find. Nothing here connects to a suggested
//! host or probes ports: only hosts that announce themselves, or that the
//! user's own configuration names, are listed.

mod bonjour;
mod render;
mod ssh_config;
mod tailscale;

use crate::HerdrWindow;
use gpui::{Context, Task};
use std::{
    net::IpAddr,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError},
    time::Duration,
};

/// How long Bonjour listens for announcements before the search is done.
const BROWSE_TIME: Duration = Duration::from_secs(5);
/// How often the form takes what the sources found so far.
const POLL: Duration = Duration::from_millis(150);
/// Machines listed at most; later reports of new machines are dropped.
const MAX_SUGGESTIONS: usize = 64;
/// Identifying names and addresses kept per machine.
const MAX_KEYS: usize = 16;
/// Display names are shortened to this many characters.
const MAX_NAME: usize = 64;
/// Sources block on `send` when the form falls this far behind.
const CHANNEL: usize = 64;

/// Where a suggestion came from. The order is the preference for the target a
/// merged suggestion fills in: the user's own alias carries their SSH options,
/// and a Tailscale name keeps working away from the local network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    SshConfig,
    Tailscale,
    Bonjour,
}

impl Source {
    const ALL: [Self; 3] = [Self::SshConfig, Self::Tailscale, Self::Bonjour];

    fn name(self) -> &'static str {
        match self {
            Self::SshConfig => "SSH config",
            Self::Tailscale => "Tailscale",
            Self::Bonjour => "Bonjour",
        }
    }
}

/// One machine as a single source reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) source: Source,
    pub(super) name: String,
    /// What the SSH target field receives.
    pub(super) target: String,
    /// Lowercase host names and addresses that identify the machine, so the
    /// same machine reported by several sources is listed once.
    pub(super) keys: Vec<String>,
}

impl Candidate {
    pub(super) fn new(source: Source, name: &str, target: String, hosts: &[&str]) -> Self {
        let mut keys = Vec::new();
        for host in hosts {
            for key in host_keys(host) {
                if !keys.contains(&key) && keys.len() < MAX_KEYS {
                    keys.push(key);
                }
            }
        }
        Self {
            source,
            name: display_name(name),
            target,
            keys,
        }
    }
}

/// A machine one or more sources reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Suggestion {
    pub(super) name: String,
    pub(super) target: String,
    /// Sorted by preference; the first one chose `name` and `target`.
    pub(super) sources: Vec<Source>,
    keys: Vec<String>,
}

impl Suggestion {
    /// Whether a saved SSH target already names this machine.
    pub(super) fn saved_as(&self, target: &str) -> bool {
        let target = target.trim();
        let target = target.strip_prefix("ssh://").unwrap_or(target);
        let host = target.rsplit_once('@').map_or(target, |(_, host)| host);
        let host = match host.strip_prefix('[') {
            Some(bracketed) => bracketed.split(']').next().unwrap_or(bracketed),
            None if host.matches(':').count() == 1 => host.split(':').next().unwrap_or(host),
            None => host,
        };
        host_keys(host).any(|key| self.keys.contains(&key))
    }
}

/// Merge a report into the list: it joins the machine it shares a name or an
/// address with, or becomes a new entry while there is room.
pub(super) fn merge(suggestions: &mut Vec<Suggestion>, candidate: Candidate) {
    if let Some(existing) = suggestions
        .iter_mut()
        .find(|existing| candidate.keys.iter().any(|key| existing.keys.contains(key)))
    {
        // Only a preferred source replaces the target; a second report from
        // the same source, such as another alias, keeps the first.
        if candidate.source < existing.sources[0] {
            existing.name = candidate.name;
            existing.target = candidate.target;
        }
        if !existing.sources.contains(&candidate.source) {
            existing.sources.push(candidate.source);
            existing.sources.sort();
        }
        for key in candidate.keys {
            if !existing.keys.contains(&key) && existing.keys.len() < MAX_KEYS {
                existing.keys.push(key);
            }
        }
    } else if suggestions.len() < MAX_SUGGESTIONS && !candidate.keys.is_empty() {
        suggestions.push(Suggestion {
            name: candidate.name,
            target: candidate.target,
            sources: vec![candidate.source],
            keys: candidate.keys,
        });
    }
    suggestions.sort_by_cached_key(|suggestion| suggestion.name.to_lowercase());
}

/// The keys a host name or address contributes: itself, lowercased without a
/// trailing dot, and for a name also its first label, which is how
/// `studio.local` and `studio.example.ts.net` are known to be one machine.
fn host_keys(host: &str) -> impl Iterator<Item = String> {
    let host = host.trim().trim_end_matches('.').to_lowercase();
    let label = (!host.is_empty() && host.parse::<IpAddr>().is_err())
        .then(|| host.split('.').next().map(str::to_owned))
        .flatten()
        .filter(|label| label != &host && !label.is_empty());
    (!host.is_empty()).then_some(host).into_iter().chain(label)
}

/// Untrusted network names become display text: control characters are
/// dropped and long names shortened.
pub(super) fn display_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// A host name safe to put in the SSH target field: letters, digits, `-`,
/// `_`, and `.`, not starting with `-` so it can never read as an option.
pub(super) fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['-', '.'])
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub(super) enum Event {
    Found(Candidate),
    Done(Source, crate::Result<()>),
}

/// Runs one source to completion, sending what it finds and then `Done`.
pub(super) type Runner = fn(Source, SyncSender<Event>);

/// Unit tests never reach the network, a Tailscale client, or the real home
/// directory: sources report nothing unless a test injects its own runner.
#[cfg(not(test))]
const RUNNER: Runner = run_source;
#[cfg(test)]
const RUNNER: Runner = |source, sender| {
    let _ = sender.send(Event::Done(source, Ok(())));
};

/// One search over every source, owned by the open form. Dropping it drops
/// the receiver, so sources still running stop at their next report.
pub(in crate::menu) struct Discovery {
    pub(super) suggestions: Vec<Suggestion>,
    /// Sources still searching.
    pub(super) pending: Vec<Source>,
    pub(super) failures: Vec<(Source, crate::Error)>,
    events: Receiver<Event>,
    _sources: Vec<Task<()>>,
    _poll: Task<()>,
}

impl Discovery {
    pub(super) fn searching(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Take every event waiting. Returns `false` once all sources are done.
    fn drain(&mut self) -> bool {
        loop {
            match self.events.try_recv() {
                Ok(Event::Found(candidate)) => merge(&mut self.suggestions, candidate),
                Ok(Event::Done(source, result)) => {
                    self.pending.retain(|pending| *pending != source);
                    if let Err(error) = result {
                        self.failures.push((source, error));
                    }
                }
                Err(TryRecvError::Empty) => return self.searching(),
                // Every source has sent `Done` before dropping its sender.
                Err(TryRecvError::Disconnected) => {
                    self.pending.clear();
                    return false;
                }
            }
        }
    }
}

/// Run one source, reporting each machine and then `Done`. A closed channel
/// means the form went away, so the source stops early.
#[cfg_attr(test, allow(dead_code))]
fn run_source(source: Source, sender: SyncSender<Event>) {
    let found = |candidate| sender.send(Event::Found(candidate)).is_ok();
    let result = match source {
        Source::SshConfig => ssh_config::hosts().map(|hosts| {
            hosts.into_iter().all(&found);
        }),
        Source::Tailscale => tailscale::peers().map(|peers| {
            peers.into_iter().all(&found);
        }),
        Source::Bonjour => bonjour::browse(BROWSE_TIME, &found),
    };
    let _ = sender.send(Event::Done(source, result));
}

impl HerdrWindow {
    /// Start a search for the open Add Device form, replacing any earlier one.
    pub(super) fn start_device_discovery(&mut self, cx: &mut Context<Self>) {
        self.start_device_discovery_with(RUNNER, cx);
    }

    pub(super) fn start_device_discovery_with(&mut self, run: Runner, cx: &mut Context<Self>) {
        let Some(setup) = &mut self.menu.device_setup else {
            return;
        };
        let (sender, events) = mpsc::sync_channel(CHANNEL);
        let sources = Source::ALL
            .into_iter()
            .map(|source| {
                let sender = sender.clone();
                cx.background_executor()
                    .spawn(async move { run(source, sender) })
            })
            .collect();
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let searching = this.update(cx, |this, cx| {
                    let searching = this
                        .menu
                        .device_setup
                        .as_mut()
                        .and_then(|setup| setup.discovery.as_mut())
                        .is_some_and(Discovery::drain);
                    cx.notify();
                    searching
                });
                if !matches!(searching, Ok(true)) {
                    break;
                }
            }
        });
        setup.discovery = Some(Discovery {
            suggestions: Vec::new(),
            pending: Source::ALL.to_vec(),
            failures: Vec::new(),
            events,
            _sources: sources,
            _poll: poll,
        });
        cx.notify();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

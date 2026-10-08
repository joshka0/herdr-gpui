use super::*;

mod form;
mod sources;

fn candidate(source: Source, name: &str, target: &str, hosts: &[&str]) -> Candidate {
    Candidate::new(source, name, target.into(), hosts)
}

#[test]
fn one_machine_reported_by_every_source_is_listed_once() {
    let mut list = Vec::new();
    merge(
        &mut list,
        candidate(
            Source::Bonjour,
            "studio",
            "studio.local",
            &["studio.local.", "192.168.1.20"],
        ),
    );
    merge(
        &mut list,
        candidate(
            Source::Tailscale,
            "studio",
            "studio.tail1.ts.net",
            &["studio.tail1.ts.net", "100.64.0.2"],
        ),
    );
    merge(
        &mut list,
        candidate(Source::SshConfig, "st", "st", &["st", "100.64.0.2"]),
    );
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].target, "st", "the user's own alias wins");
    assert_eq!(list[0].name, "st");
    assert_eq!(
        list[0].sources,
        [Source::SshConfig, Source::Tailscale, Source::Bonjour]
    );
}

#[test]
fn a_less_preferred_source_does_not_replace_the_target() {
    let mut list = Vec::new();
    merge(
        &mut list,
        candidate(
            Source::Tailscale,
            "box",
            "box.tail1.ts.net",
            &["box.tail1.ts.net"],
        ),
    );
    merge(
        &mut list,
        candidate(Source::Bonjour, "box", "box.local", &["box.local"]),
    );
    assert_eq!(list[0].target, "box.tail1.ts.net");
    assert_eq!(list[0].sources, [Source::Tailscale, Source::Bonjour]);
    // A second alias for the same machine does not rename the first.
    merge(
        &mut list,
        candidate(Source::Tailscale, "box.lan", "box.lan", &["box.lan"]),
    );
    assert_eq!(list[0].target, "box.tail1.ts.net");
}

#[test]
fn different_machines_stay_apart_sorted_by_name() {
    let mut list = Vec::new();
    for name in ["zeta", "Alpha", "mid"] {
        let host = format!("{name}.local");
        merge(
            &mut list,
            candidate(Source::Bonjour, name, &host, &[host.as_str()]),
        );
    }
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Alpha", "mid", "zeta"]);
}

#[test]
fn the_list_is_bounded() {
    let mut list = Vec::new();
    for index in 0..MAX_SUGGESTIONS + 10 {
        let host = format!("host{index}.local");
        merge(
            &mut list,
            candidate(Source::Bonjour, &host, &host, &[host.as_str()]),
        );
    }
    assert_eq!(list.len(), MAX_SUGGESTIONS);
}

#[test]
fn a_saved_target_hides_its_machine() {
    let mut list = Vec::new();
    merge(
        &mut list,
        candidate(
            Source::Tailscale,
            "box",
            "box.tail1.ts.net",
            &["box.tail1.ts.net", "100.64.0.9"],
        ),
    );
    let suggestion = &list[0];
    for saved in [
        "box",
        "me@box",
        "ssh://me@box.tail1.ts.net:2222",
        "BOX.tail1.ts.net",
        "100.64.0.9",
        "ssh://[100.64.0.9]:22",
    ] {
        assert!(suggestion.saved_as(saved), "{saved}");
    }
    for other in ["boxer", "me@other", "100.64.0.10"] {
        assert!(!suggestion.saved_as(other), "{other}");
    }
}

#[test]
fn network_names_are_bounded_display_text() {
    assert_eq!(display_name(" a\u{1b}[31mb\n "), "a[31mb");
    assert_eq!(display_name(&"x".repeat(200)).len(), MAX_NAME);
}

#[test]
fn only_plain_host_names_become_targets() {
    for host in ["studio.local", "box-1", "a_b.example"] {
        assert!(valid_host(host), "{host}");
    }
    for host in ["", "-oProxyCommand=x", ".local", "a b", "a;b", "host$(x)"] {
        assert!(!valid_host(host), "{host}");
    }
}

#[test]
fn source_lists_read_as_prose() {
    use super::render::sources;
    assert_eq!(sources(&[Source::Bonjour]), "Bonjour");
    assert_eq!(
        sources(&[Source::Tailscale, Source::Bonjour]),
        "Tailscale and Bonjour"
    );
    assert_eq!(sources(&Source::ALL), "SSH config, Tailscale, and Bonjour");
}

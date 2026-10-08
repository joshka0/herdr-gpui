#![allow(clippy::unwrap_used)]
use super::*;

fn device(id: &str) -> SavedDevice {
    SavedDevice {
        provider: CloudProvider::Coder,
        id: id.into(),
        label: "Dev box".into(),
        account: "https://coder.example.com".into(),
        machine: "herdr-dev-box".into(),
        session: "default".into(),
        enabled: true,
    }
}

#[test]
fn saves_replace_by_provider_and_id_and_survive_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join(FILE);
    assert!(read(&path).unwrap().is_empty());
    save_in(&path, device("w1")).unwrap();
    save_in(&path, device("w2")).unwrap();
    let mut renamed = device("w1");
    renamed.label = "Renamed".into();
    save_in(&path, renamed.clone()).unwrap();
    let saved = read(&path).unwrap();
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0], renamed);
    assert_eq!(saved[0].endpoint_id(), "coder:w1");
    assert_eq!(
        saved[0].target(),
        ConnectTarget::Cloud {
            provider: CloudProvider::Coder,
            account: "https://coder.example.com".into(),
            machine: "herdr-dev-box".into(),
            session: "default".into(),
        }
    );
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("\"provider\": \"coder\""), "{text}");
}

#[test]
fn invalid_or_oversized_documents_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    for bad in [
        SavedDevice {
            id: "../x".into(),
            ..device("w")
        },
        SavedDevice {
            machine: "-oProxyCommand=x".into(),
            ..device("w")
        },
        SavedDevice {
            machine: "a b".into(),
            ..device("w")
        },
        SavedDevice {
            session: "a/b".into(),
            ..device("w")
        },
        SavedDevice {
            label: "line\nbreak".into(),
            ..device("w")
        },
        SavedDevice {
            account: String::new(),
            ..device("w")
        },
    ] {
        assert!(write(&path, vec![bad]).is_err());
    }
    for text in [
        r#"{"version":2,"devices":[]}"#,
        r#"{"version":1,"devices":[],"extra":1}"#,
        r#"{"version":1,"devices":[{"provider":"nimbus","id":"w","label":"l","account":"a","machine":"m","session":"default","enabled":true}]}"#,
        "not json",
    ] {
        fs::write(&path, text).unwrap();
        assert!(read(&path).is_err(), "{text}");
    }
    fs::write(&path, vec![b' '; LIMIT as usize + 1]).unwrap();
    assert!(read(&path).is_err());
    let many = (0..=MAX_DEVICES)
        .map(|i| device(&format!("w{i}")))
        .collect();
    assert!(write(&path, many).is_err());
}

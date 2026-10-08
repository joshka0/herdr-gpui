use super::*;

#[test]
fn the_vs_code_server_is_one_web_address() -> anyhow::Result<()> {
    assert_eq!(Config::parse("")?.code.url, None);
    assert_eq!(Config::parse(DEFAULT_CONFIG)?.code.url, None);
    let config = Config::parse("[code]\nurl = \"http://127.0.0.1:8000/?tkn=x\"")?;
    assert_eq!(
        config.code.url.as_ref().map(crate::browser::WebUrl::as_str),
        Some("http://127.0.0.1:8000/?tkn=x")
    );
    for invalid in [
        "[code]\nurl = \"file:///etc/passwd\"",
        "[code]\nurl = \"127.0.0.1:8000\"",
        "[code]\nurl = 8000",
        "[code]\nwidth = 400",
    ] {
        assert!(Config::parse(invalid).is_err(), "{invalid}");
    }
    Ok(())
}

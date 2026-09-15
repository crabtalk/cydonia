use super::loopback_bypass;

#[test]
fn loopback_is_exempt_without_existing_bypasses() {
    assert_eq!(loopback_bypass([None, None]), "localhost,127.0.0.1,::1");
}

#[test]
fn both_proxy_bypass_lists_are_preserved() {
    assert_eq!(
        loopback_bypass([
            Some(" .example.com, localhost, ".into()),
            Some("10.0.0.0/8,localhost".into()),
        ]),
        ".example.com,localhost,10.0.0.0/8,127.0.0.1,::1"
    );
    assert_eq!(
        loopback_bypass([Some("*".into()), None]),
        "*,localhost,127.0.0.1,::1"
    );
}

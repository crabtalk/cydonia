use cacp_agents::mcp::{self, Distribution};

#[test]
fn collapses_repeated_versions_of_one_server() {
    let servers = mcp::parse(
        r#"{"servers":[
            {"server":{"name":"a/b","description":"newest",
              "packages":[{"registryType":"npm","identifier":"b","version":"2.0.0"}]}},
            {"server":{"name":"a/b","description":"older",
              "packages":[{"registryType":"npm","identifier":"b","version":"1.0.0"}]}}
        ]}"#,
    )
    .expect("parses");
    assert_eq!(servers.len(), 1);
    assert_eq!(servers[0].description, "newest");
}

#[test]
fn prefers_stdio_packages_over_remotes() {
    let servers = mcp::parse(
        r#"{"servers":[
            {"server":{"name":"io.github.acme/files","title":"Files","description":"d",
              "packages":[{"registryType":"npm","identifier":"files-mcp","version":"1.2.3"}],
              "remotes":[{"type":"streamable-http","url":"https://example.invalid/mcp"}]}}
        ]}"#,
    )
    .expect("parses");
    assert!(matches!(
        &servers[0].distribution,
        Distribution::Npm { package } if package == "files-mcp@1.2.3"
    ));
}

#[test]
fn falls_back_to_remote_and_flags_the_rest() {
    let servers = mcp::parse(
        r#"{"servers":[
            {"server":{"name":"ac.inference.sh/mcp","description":"d",
              "remotes":[{"type":"streamable-http","url":"https://api.example.invalid/mcp"}]}},
            {"server":{"name":"x.y/z","description":"d",
              "packages":[{"registryType":"pypi","identifier":"zzz"}]}}
        ]}"#,
    )
    .expect("parses");
    assert!(servers[0].is_remote());
    // No title: the display name is the last path segment.
    assert_eq!(servers[0].name, "mcp");
    assert!(!servers[1].installable());
}

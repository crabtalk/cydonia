//! What an install is agreed to: the source, which outlives a version.

use super::*;
use cacp_agents::registry::{Agent, Binary, Distribution};

fn npm(id: &str, package: &str, version: &str) -> Agent {
    Agent {
        id: id.to_owned(),
        name: id.to_owned(),
        version: version.to_owned(),
        description: None,
        repository: None,
        website: None,
        icon: None,
        distribution: Distribution::Npm {
            package: package.to_owned(),
            args: Vec::new(),
        },
    }
}

/// A new release of the same package is the same decision, so the mark a
/// version bump produces is the one already written down.
#[test]
fn a_release_moves_and_the_mark_does_not() {
    let before = source_mark(&npm("claude", "@anthropic/claude-code-acp@1.2.0", "1.2.0"));
    let after = source_mark(&npm("claude", "@anthropic/claude-code-acp@1.3.0", "1.3.0"));
    assert_eq!(before, after);
    assert_eq!(before, "npm:@anthropic/claude-code-acp");
}

/// A different package under the same id is a different publisher's code, and
/// is not what was agreed to.
#[test]
fn a_package_that_changed_is_a_fresh_decision() {
    let was = source_mark(&npm("claude", "@anthropic/claude-code-acp", "1.2.0"));
    let now = source_mark(&npm("claude", "@somebody-else/claude-acp", "1.2.0"));
    assert_ne!(was, now);
}

/// A download is agreed to by the host that serves it: the path moves with
/// every release, and the host is who is trusted.
#[test]
fn a_download_is_marked_by_its_host() {
    let agent = |archive: &str| Agent {
        distribution: Distribution::Binary(Binary {
            archive: archive.to_owned(),
            cmd: "bin/agent".to_owned(),
            sha256: None,
            args: Vec::new(),
            env: Default::default(),
        }),
        ..npm("proprietary", "unused", "1.0.0")
    };
    let first = source_mark(&agent("https://dl.example.com/agent/v1.0.0/mac.tar.gz"));
    let later = source_mark(&agent("https://dl.example.com/agent/v2.0.0/mac.tar.gz"));
    assert_eq!(first, later);
    assert_eq!(first, "binary:dl.example.com");
    assert_ne!(
        first,
        source_mark(&agent("https://elsewhere.example/agent/v1.0.0/mac.tar.gz"))
    );
}

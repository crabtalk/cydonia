#[path = "../build/bundle.rs"]
mod bundle;

use std::{fs, path::PathBuf};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("cydonia-resources-{name}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn resource(&self, folder: &str, content: &str) {
        fs::write(self.0.join(format!("{folder}.md")), content).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn new_folders_are_discovered_with_yaml_metadata_and_stable_order() {
    let scratch = Scratch::new("discovery");
    scratch.resource(
        "z-last",
        "---\nname: z-last\ndescription: >-\n  A folded\n  description.\n---\nUse this.\n",
    );
    scratch.resource(
        "a-first",
        "---\nname: a-first\ndescription: 'Quoted: description'\n---\nKeep \\\"literal\\\" text.\n",
    );
    let generated = bundle::generate(&scratch.0).unwrap();
    assert!(generated.find("a-first").unwrap() < generated.find("z-last").unwrap());
    assert!(generated.contains("A folded description."));
    assert!(generated.contains("Quoted: description"));
    assert_eq!(generated, bundle::generate(&scratch.0).unwrap());
}

#[test]
fn invalid_metadata_fails_the_build() {
    let scratch = Scratch::new("invalid");
    for content in [
        "No frontmatter",
        "---\nname: example\n---\nBody",
        "---\nname: different\ndescription: Example\n---\nBody",
        "---\nname: example\ndescription: ''\n---\nBody",
        "---\nname: example\ndescription: Example\n---\n",
    ] {
        scratch.resource("example", content);
        assert!(bundle::generate(&scratch.0).is_err(), "{content}");
    }
}

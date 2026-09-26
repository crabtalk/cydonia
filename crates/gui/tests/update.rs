//! What the updater decides before it touches anything: which version is worth
//! fetching, and where the image for it is.

use cydonia_gui::model::update;

/// The ordering, which is the whole of what decides that an app replaces
/// itself.
#[test]
fn a_later_release_is_the_only_one_offered() {
    assert!(update::newer("0.1.4", "0.1.3"));
    assert!(update::newer("0.2.0", "0.1.9"));
    assert!(update::newer("1.0.0", "0.9.9"));

    // The version that is running is not a release to move to, and neither is
    // one behind it — a feed rolled back must not walk an install backwards.
    assert!(!update::newer("0.1.3", "0.1.3"));
    assert!(!update::newer("0.1.2", "0.1.3"));
    assert!(!update::newer("0.9.9", "1.0.0"));
}

/// Three numbers is the whole grammar. Anything else is a string this cannot
/// order, and the answer to one of those is to leave the app alone rather than
/// to guess at it.
#[test]
fn a_version_this_cannot_order_is_not_offered() {
    assert!(!update::newer("0.1.4-rc1", "0.1.3"));
    assert!(!update::newer("0.1.4", "0.1.3-rc1"));
    assert!(!update::newer("0.2", "0.1.3"));
    assert!(!update::newer("0.1.4.1", "0.1.3"));
    assert!(!update::newer("", "0.1.3"));
    assert!(!update::newer("latest", "0.1.3"));
}

/// The image's name, which the Makefile writes and nothing checks at runtime:
/// `arm64` is Apple's spelling, and a build that asked for rustc's `aarch64`
/// would ask GitHub for a file no release has ever carried.
#[cfg(target_os = "macos")]
#[test]
fn the_image_is_named_as_the_makefile_names_it() {
    assert_eq!(update::asset("0.1.4"), "cydonia-0.1.4-arm64.dmg");
}

/// And the address it is published at, which has to be the same one the
/// download button on the site builds — `www/src/lib/meta.js`, `dmgFor`.
#[cfg(target_os = "macos")]
#[test]
fn the_release_is_where_the_site_says_it_is() {
    assert_eq!(
        update::url("0.1.4"),
        "https://github.com/crabtalk/cydonia/releases/download/v0.1.4/cydonia-0.1.4-arm64.dmg"
    );
}

/// The tarball carries no version on the release, the name `make tarball`
/// writes and `www/src/lib/meta.js` links; the tag is what says which one.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn the_tarball_is_under_the_versions_tag() {
    assert_eq!(
        update::url("0.1.4"),
        "https://github.com/crabtalk/cydonia/releases/download/v0.1.4/cydonia-linux-x86_64.tar.gz"
    );
}

/// The installer, as `bundle/windows/cydonia.iss` names it.
#[cfg(windows)]
#[test]
fn the_installer_is_under_the_versions_tag() {
    assert_eq!(
        update::url("0.1.4"),
        "https://github.com/crabtalk/cydonia/releases/download/v0.1.4/cydonia-windows-x86_64-setup.exe"
    );
}

/// The feed is built out of the homepage, so this is the check that the two
/// stay one address: the file is prerendered at the site's root by
/// `www/src/routes/changelog.json/+server.js`.
#[test]
fn the_feed_is_a_file_at_the_site_root() {
    assert_eq!(update::FEED, "https://cydonia.sh/changelog.json");
}

/// The name goes both ways, which is what makes the sweep safe: a file it reads
/// no version out of is one it deletes.
#[test]
fn an_image_names_its_own_version() {
    let image = std::path::PathBuf::from(update::asset("0.1.4"));
    assert_eq!(update::version_of(&image).as_deref(), Some("0.1.4"));

    // Not ours, or not finished. Either way not an image to keep.
    let part = std::path::PathBuf::from(update::asset("0.1.4")).with_extension("part");
    for name in [
        "cydonia-0.1.4-x86_64.dmg",
        part.to_str().unwrap(),
        "cydonia-",
        "mnt",
    ] {
        let path = std::path::PathBuf::from(name);
        assert_eq!(update::version_of(&path), None, "{name}");
    }
}

/// The feed the app reads is the file in this repository, so the shape it
/// expects is checked against it rather than against a sample.
#[test]
fn the_shipped_changelog_is_a_feed_this_build_can_read() {
    let feed = include_str!("../../../changelog.json");
    let head = update::head(feed).expect("changelog.json is the feed's shape");
    assert_eq!(head, env!("CARGO_PKG_VERSION"));
}

/// The predicate that deletes directories beside the app, which is the one
/// place this reaches outside its own cache.
#[test]
fn only_a_spent_staging_directory_of_ours_is_swept() {
    let version = env!("CARGO_PKG_VERSION");
    assert!(update::spent(&format!(".cydonia-update-{version}")));
    assert!(update::spent(".cydonia-update-0.0.1"));

    // A release still ahead of this build is one to restart into, not to throw
    // away — and everything else in that directory belongs to somebody else.
    assert!(!update::spent(".cydonia-update-99.0.0"));
    assert!(!update::spent("cydonia.app"));
    assert!(!update::spent("Safari.app"));
    assert!(!update::spent(".DS_Store"));
}

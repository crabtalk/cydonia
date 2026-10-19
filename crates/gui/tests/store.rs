//! A path seeded with a memory project is answered from memory, and nothing
//! lands on the disk there.

use artifact::location::Location;
use artifact::project::{Project as _, memory};
use cydonia_gui::model::{store, welcome};

#[test]
fn a_seeded_path_reads_the_seed() {
    let path = std::env::temp_dir().join(format!("cydonia-seeded-{}", std::process::id()));
    let location = Location::Local(path.clone());
    store::seed(location.clone(), memory::Project::seed(welcome::files()));
    let opened = store::open(&location);
    assert!(matches!(opened, store::Store::Memory(_)));
    assert_eq!(opened.boards().len(), 1);
    assert_eq!(opened.articles().len(), 2);

    let made = futures::executor::block_on(opened.create_article("# Scratch\n")).unwrap();
    assert_eq!(
        store::open(&location).read_article(&made.id).unwrap(),
        "# Scratch\n"
    );
    assert!(!path.exists(), "a memory project writes nothing to disk");
}

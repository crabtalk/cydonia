use super::*;
use std::os::unix::ffi::OsStrExt as _;

#[test]
fn porcelain_preserves_non_utf8_paths_and_rename_record_boundaries() {
    let files = parse_status(b"R  new-\xff\0old-\xff\0?? next\0").unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].path.as_os_str().as_bytes(), b"new-\xff");
    assert_eq!(
        files[0].original.as_ref().unwrap().as_os_str().as_bytes(),
        b"old-\xff"
    );
    assert_eq!(files[1].path, Path::new("next"));
}

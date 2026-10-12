use cydonia_artifact::{
    article::comments::{self, Comment, Comments},
    document,
};

fn held(text: &str, quote: &str) -> Comments {
    let start = text.find(quote).unwrap();
    let mut held = Comments {
        comments: vec![Comment::new(
            1,
            text,
            start..start + quote.len(),
            "note".into(),
            None,
        )],
        ..Comments::default()
    };
    held.stamp(text);
    held
}

fn quoted<'a>(text: &'a str, held: &Comments) -> &'a str {
    &text[held.comments[0].range()]
}

#[test]
fn remap_follows_text_inserted_before() {
    let old = "alpha beta gamma";
    let new = "zero alpha beta gamma";
    let mut held = held(old, "beta");
    held.remap(old, new);
    assert_eq!(quoted(new, &held), "beta");
    assert!(held.fits(new));
}

#[test]
fn remap_keeps_an_edit_inside_the_range() {
    let old = "alpha beta gamma";
    let new = "alpha betta gamma";
    let mut held = held(old, "beta");
    held.remap(old, new);
    assert_eq!(quoted(new, &held), "betta");
}

#[test]
fn remap_detaches_a_range_whose_text_went() {
    let old = "alpha beta gamma";
    let new = "alpha gamma";
    let mut held = held(old, "beta ");
    held.remap(old, new);
    assert!(held.comments[0].detached());
}

#[test]
fn refind_picks_the_match_with_its_context() {
    let old = "one cat here. two cat there.";
    let mut held = held(old, "cat there");
    held.comments[0] = Comment::new(1, old, 18..21, "note".into(), None);
    let new = "intro. one cat here. two cat there.";
    held.refind(new);
    assert_eq!(held.comments[0].start, new.rfind("cat").unwrap());
}

#[test]
fn refind_detaches_a_quote_that_is_gone() {
    let old = "alpha beta gamma";
    let mut held = held(old, "beta");
    held.refind("alpha gamma");
    assert!(held.comments[0].detached());
}

#[test]
fn document_write_carries_comments_beside_it() {
    let dir = std::env::temp_dir().join(format!("cydonia-comments-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let content = document::content(&dir);
    let old = "alpha beta gamma";
    std::fs::write(&content, old).unwrap();
    comments::save(&content, old, &held(old, "gamma")).unwrap();

    let new = "alpha beta and then gamma";
    document::write(&content, new).unwrap();
    let read = comments::read(&content);
    assert!(read.fits(new));
    assert_eq!(quoted(new, &read), "gamma");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_file_edited_elsewhere_is_refound_on_read() {
    let dir =
        std::env::temp_dir().join(format!("cydonia-comments-elsewhere-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let content = document::content(&dir);
    let old = "alpha beta gamma";
    std::fs::write(&content, old).unwrap();
    comments::save(&content, old, &held(old, "beta")).unwrap();

    let new = "new start. alpha beta gamma";
    std::fs::write(&content, new).unwrap();
    let mut read = comments::read(&content);
    read.carry(None, new);
    assert_eq!(quoted(new, &read), "beta");
    std::fs::remove_dir_all(&dir).unwrap();
}

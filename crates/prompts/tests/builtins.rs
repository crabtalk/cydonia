#[test]
fn bundled_skills_are_sorted_and_resolve_without_a_checkout() {
    let all = cydonia_prompts::skills::list();
    assert!(!all.is_empty());
    assert!(all.windows(2).all(|pair| pair[0].name < pair[1].name));
    for skill in all {
        assert_eq!(
            cydonia_prompts::skills::read(skill.name).unwrap().content,
            skill.content
        );
        assert!(cydonia_prompts::skills::catalog().contains(skill.description));
        assert!(skill.content.starts_with("---\n"));
    }
    assert!(cydonia_prompts::skills::read("../../SKILL.md").is_none());
}

#[test]
fn catalog_defers_full_instructions() {
    let skill = cydonia_prompts::skills::read("cydonia-markdown").unwrap();
    assert!(skill.content.contains("![Architecture overview|480]"));
    assert!(!cydonia_prompts::skills::catalog().contains("![Architecture overview|480]"));
}

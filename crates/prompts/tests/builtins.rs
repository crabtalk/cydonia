#[test]
fn bundled_resources_are_sorted_and_resolve_without_a_checkout() {
    let all = cydonia_prompts::resources::list();
    assert!(!all.is_empty());
    assert!(all.windows(2).all(|pair| pair[0].name < pair[1].name));
    for resource in all {
        assert_eq!(
            cydonia_prompts::resources::read(resource.name)
                .unwrap()
                .content,
            resource.content
        );
        assert!(cydonia_prompts::resources::catalog().contains(resource.description));
        assert!(resource.content.starts_with("---\n"));
    }
    assert!(cydonia_prompts::resources::read("../../SKILL.md").is_none());
}

#[test]
fn catalog_defers_full_instructions() {
    let resource = cydonia_prompts::resources::read("markdown").unwrap();
    assert!(resource.content.contains("![Architecture overview|480]"));
    assert!(!cydonia_prompts::resources::catalog().contains("![Architecture overview|480]"));
}

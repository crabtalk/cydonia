use super::*;

fn fixture() -> Bundle {
    Bundle {
        wasm: STANDARD.encode(include_bytes!("../fixtures/grammar/json.wasm")),
        queries: vec![include_str!("../fixtures/grammar/json.scm").into()],
    }
}

#[test]
fn cached_grammar_round_trips_and_corruption_is_rejected() {
    super::super::installed();
    let spec = spec("json").unwrap();
    let root = std::env::temp_dir().join(format!("cydonia-grammar-test-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    assert!(read_cached(spec, &root).unwrap().is_none());
    save(spec, &root, &fixture()).unwrap();
    let (wasm, query) = read_cached(spec, &root).unwrap().unwrap();
    validate(spec, &wasm, &query).unwrap();
    register(spec, Some((wasm, query)));
    let spans = super::super::highlight("json", r#"{"name": "cydonia", "n": 42}"#).unwrap();
    assert!(!spans.is_empty());
    let mut bad = fixture();
    bad.queries[0].push(' ');
    save(spec, &root, &bad).unwrap();
    assert!(read_cached(spec, &root).is_err());
    assert!(
        !root
            .join(format!(".json-{}.part", std::process::id()))
            .exists()
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn incomplete_or_tampered_downloads_are_rejected() {
    let spec = spec("json").unwrap();
    let mut bundle = fixture();
    bundle.queries.clear();
    assert!(unpack(spec, &bundle).is_err());
    let mut bytes = include_bytes!("../fixtures/grammar/json.wasm").to_vec();
    bytes[0] ^= 1;
    assert!(verify(&spec.wasm, &bytes).is_err());
    assert!(verify(&spec.wasm, &bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn invalid_modules_and_queries_cannot_be_installed() {
    let spec = spec("json").unwrap();
    assert!(validate(spec, b"invalid", "(string) @string").is_err());
    assert!(
        validate(
            spec,
            include_bytes!("../fixtures/grammar/json.wasm"),
            "(nonexistent_node) @string"
        )
        .is_err()
    );
}

#[test]
fn catalog_has_unique_names_and_bounded_downloads() {
    let mut names = std::collections::HashSet::new();
    for spec in catalog() {
        assert!(names.insert(&spec.name));
        assert!(spec.aliases.contains(&spec.name));
        assert!(spec.aliases.contains(&spec.symbol));
        assert!(!spec.queries.is_empty());
        for asset in std::iter::once(&spec.wasm).chain(&spec.queries) {
            assert!(asset.size > 0 && asset.size < MAX_CACHE / 2);
            assert_eq!(asset.sha256.len(), 64);
            assert!(!asset.path.contains(".."));
        }
    }
}

/// Run when updating the pinned catalog, against the unpacked npm package.
#[test]
#[ignore = "requires CYDONIA_GRAMMAR_PACKAGE pointing to package/out"]
fn all_catalog_grammars_load_and_queries_compile() {
    let root = PathBuf::from(std::env::var("CYDONIA_GRAMMAR_PACKAGE").unwrap());
    let mut failures = Vec::new();
    for spec in catalog() {
        let wasm = std::fs::read(root.join(&spec.wasm.path)).unwrap();
        verify(&spec.wasm, &wasm).unwrap();
        let queries = spec
            .queries
            .iter()
            .map(|asset| {
                let bytes = std::fs::read(root.join(&asset.path)).unwrap();
                verify(asset, &bytes).unwrap();
                String::from_utf8(bytes).unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if let Err(error) = validate(spec, &wasm, &queries) {
            failures.push(format!("{}: {error:#}", spec.name));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn concurrent_install_requests_share_progress_and_failures_allow_retry() {
    super::super::installed();
    let spec = spec("json").unwrap();
    set_status("json", Status::Missing);
    assert!(begin_install(spec));
    assert!(!begin_install(spec));
    set_status("json", Status::Checking);
    assert!(!begin_install(spec));
    set_status("json", Status::Failed("offline".into()));
    assert!(begin_install(spec));
    set_status("json", Status::Ready);
    assert!(!begin_install(spec));
}

#[test]
fn interrupted_install_does_not_publish_a_grammar_and_retry_succeeds() {
    super::super::installed();
    let spec = spec("json").unwrap();
    let root = std::env::temp_dir().join(format!("cydonia-install-test-{}", std::process::id()));
    let wasm = include_bytes!("../fixtures/grammar/json.wasm");
    let result = install_with(spec, &root, |asset, progress| {
        if asset.path.ends_with(".scm") {
            anyhow::bail!("connection lost");
        }
        progress(wasm.len() as u64);
        Ok(wasm.to_vec())
    });
    assert!(result.is_err());
    assert!(read_cached(spec, &root).unwrap().is_none());
    assert!(registry::of_tag("json").unwrap().lang().is_none());
    install_with(spec, &root, |asset, progress| {
        let bytes = if asset.path.ends_with(".wasm") {
            wasm.to_vec()
        } else {
            include_bytes!("../fixtures/grammar/json.scm").to_vec()
        };
        progress(bytes.len() as u64);
        Ok(bytes)
    })
    .unwrap();
    assert!(read_cached(spec, &root).unwrap().is_some());
    assert!(registry::of_tag("json").unwrap().lang().is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "downloads the pinned JSON grammar from jsDelivr"]
fn pinned_download_installs_and_highlights() {
    super::super::installed();
    let spec = spec("json").unwrap();
    let root = std::env::temp_dir().join(format!("cydonia-grammar-network-{}", std::process::id()));
    install(spec, &root).unwrap();
    let (wasm, query) = read_cached(spec, &root).unwrap().unwrap();
    validate(spec, &wasm, &query).unwrap();
    assert!(
        !super::super::highlight("json", r#"{"works": true}"#)
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(root).unwrap();
}

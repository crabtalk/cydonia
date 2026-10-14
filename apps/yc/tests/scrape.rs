//! Reading yc-oss's JSON and a company's page.

use yc::scrape::{Batch, Company, image, page_props, unescape};

const META: &str = include_str!("fixtures/meta.json");

const LISTING: &str = include_str!("fixtures/listing.json");
const PAGE: &str = include_str!("fixtures/page.html");

#[test]
fn batches_are_listed_newest_first_with_no_season_last() {
    let batches = Batch::list(META).unwrap();
    let slugs: Vec<&str> = batches.iter().map(|batch| batch.slug.as_str()).collect();
    assert_eq!(
        slugs,
        [
            "fall-2025",
            "spring-2025",
            "winter-2025",
            "summer-2011",
            "unspecified"
        ]
    );
    assert_eq!(batches[2].count, 166);
    assert_eq!(
        batches[2].url(),
        "https://yc-oss.github.io/api/batches/winter-2025.json"
    );
}

#[test]
fn a_page_adds_its_place_partner_and_founders_to_a_company() {
    let mut companies = Company::list("winter-2025", LISTING).unwrap();
    assert_eq!(companies.len(), 2);
    let company = &mut companies[0];
    assert_eq!(company.batch, "winter-2025");
    assert_eq!(company.tags, "AI, Dental");
    assert_eq!(company.team_size, Some(2));
    let founders = company.read_page(PAGE).unwrap();
    assert_eq!(
        (
            company.city.as_str(),
            company.country.as_str(),
            company.year_founded
        ),
        ("San Francisco", "US", Some(2024))
    );
    assert_eq!(company.partner, "Harj Taggar");
    assert_eq!(founders.len(), 1);
    let founder = &founders[0];
    assert_eq!(
        (founder.company.as_str(), founder.batch.as_str()),
        ("egress-health", "winter-2025")
    );
    assert_eq!(founder.bio, "Prev Microsoft & Harvard");
    assert_eq!(founder.x, "");
    assert_eq!(
        founder.avatar,
        "https://bookface-images.s3.amazonaws.com/avatars/7456.jpg"
    );
}

#[test]
fn a_page_without_its_props_is_an_error() {
    let mut company = Company::default();
    assert!(company.read_page("<html></html>").is_err());
    assert!(page_props("<div data-page=\"{not json\">").is_err());
}

#[test]
fn images_are_kept_on_the_one_host_without_their_signature() {
    assert_eq!(
        image(
            "https://bookface-images.s3.us-west-2.amazonaws.com/avatars/a.jpg?X-Amz-Expires=3600"
        ),
        "https://bookface-images.s3.amazonaws.com/avatars/a.jpg"
    );
    assert_eq!(image("https://elsewhere.example/a.png"), "");
    assert_eq!(image(""), "");
}

#[test]
fn character_references_are_undone() {
    assert_eq!(
        unescape("a &amp; b &lt;c&gt; &quot;d&quot; &#39;e&#x27; & f &bogus;"),
        "a & b <c> \"d\" 'e' & f &bogus;"
    );
}

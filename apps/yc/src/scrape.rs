//! Reading yc-oss's JSON and a company's page on www.ycombinator.com.

use serde_json::Value;

/// yc-oss's API.
pub const YC_OSS: &str = "https://yc-oss.github.io/api";
/// The host every logo and avatar is fetched from.
const IMAGES: &str = "bookface-images.s3.amazonaws.com";

/// A batch, as yc-oss lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Batch {
    /// `winter-2025`.
    pub slug: String,
    /// `Winter 2025`.
    pub name: String,
    pub count: u64,
}

impl Batch {
    /// The batch list in `meta.json`, newest first, with the batches of no
    /// season last.
    pub fn list(meta: &str) -> Result<Vec<Batch>, String> {
        let meta: Value = serde_json::from_str(meta).map_err(|error| error.to_string())?;
        let batches = meta["batches"]
            .as_object()
            .ok_or("meta.json lists no batches")?;
        let mut batches: Vec<Batch> = batches
            .iter()
            .map(|(slug, batch)| Batch {
                slug: slug.clone(),
                name: batch["name"].as_str().unwrap_or(slug).to_owned(),
                count: batch["count"].as_u64().unwrap_or_default(),
            })
            .collect();
        batches.sort_by_key(|batch| std::cmp::Reverse(batch.order()));
        Ok(batches)
    }

    /// The batch's JSON on yc-oss.
    pub fn url(&self) -> String {
        format!("{YC_OSS}/batches/{}.json", self.slug)
    }

    /// Year, then season within it. Zero for a batch with no season.
    fn order(&self) -> (u32, u8) {
        let Some((season, year)) = self.slug.rsplit_once('-') else {
            return (0, 0);
        };
        let season = match season {
            "winter" => 1,
            "spring" => 2,
            "summer" => 3,
            "fall" => 4,
            _ => 0,
        };
        (year.parse().unwrap_or_default(), season)
    }
}

/// A company: yc-oss's record, and what its YC page adds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Company {
    pub slug: String,
    pub name: String,
    /// The batch's slug.
    pub batch: String,
    pub batch_name: String,
    pub one_liner: String,
    pub website: String,
    /// Its page on www.ycombinator.com.
    pub url: String,
    pub logo: String,
    pub status: String,
    pub team_size: Option<i64>,
    pub industry: String,
    pub tags: String,
    pub city: String,
    pub country: String,
    pub year_founded: Option<i64>,
    pub partner: String,
}

impl Company {
    /// The companies in a batch's JSON on yc-oss.
    pub fn list(batch: &str, json: &str) -> Result<Vec<Company>, String> {
        let entries: Vec<Value> = serde_json::from_str(json).map_err(|error| error.to_string())?;
        Ok(entries
            .iter()
            .map(|entry| Company {
                slug: text(&entry["slug"]),
                name: text(&entry["name"]),
                batch: batch.to_owned(),
                batch_name: text(&entry["batch"]),
                one_liner: text(&entry["one_liner"]),
                website: text(&entry["website"]),
                url: text(&entry["url"]),
                logo: image(&text(&entry["small_logo_thumb_url"])),
                status: text(&entry["status"]),
                team_size: entry["team_size"].as_i64(),
                industry: text(&entry["industry"]),
                tags: entry["tags"]
                    .as_array()
                    .map(|tags| {
                        tags.iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default(),
                ..Company::default()
            })
            .collect())
    }

    /// Take what the company's page says, and answer with its founders.
    pub fn read_page(&mut self, html: &str) -> Result<Vec<Founder>, String> {
        let props = page_props(html)?;
        let company = &props["props"]["company"];
        if !company.is_object() {
            return Err("the page carries no company".into());
        }
        self.city = text(&company["city"]);
        self.country = text(&company["country"]);
        self.year_founded = company["year_founded"].as_i64();
        self.partner = text(&company["primary_group_partner"]["full_name"]);
        Ok(company["founders"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|founder| Founder {
                company: self.slug.clone(),
                batch: self.batch.clone(),
                name: text(&founder["full_name"]),
                title: text(&founder["title"]),
                bio: text(&founder["founder_bio"]),
                linkedin: text(&founder["linkedin_url"]),
                x: text(&founder["twitter_url"]),
                avatar: image(&text(&founder["avatar_thumb_url"])),
            })
            .collect())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Founder {
    /// The company's slug.
    pub company: String,
    /// The batch's slug.
    pub batch: String,
    pub name: String,
    pub title: String,
    pub bio: String,
    pub linkedin: String,
    pub x: String,
    pub avatar: String,
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().trim().to_owned()
}

/// A logo's or an avatar's URL as it is kept: on [`IMAGES`], with no query.
/// An avatar's URL is presigned and expires within the hour; the object is
/// public without the signature.
pub fn image(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://") else {
        return String::new();
    };
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !host.starts_with("bookface-images.s3") {
        return String::new();
    }
    let path = path.split(['?', '#']).next().unwrap_or_default();
    format!("https://{IMAGES}/{path}")
}

/// The props an Inertia page carries: the HTML-escaped JSON in its
/// `data-page` attribute.
pub fn page_props(html: &str) -> Result<Value, String> {
    let start = html
        .find("data-page=\"")
        .ok_or("the page has no data-page")?
        + "data-page=\"".len();
    let end = html[start..]
        .find('"')
        .ok_or("the page's data-page does not end")?;
    serde_json::from_str(&unescape(&html[start..start + end])).map_err(|error| error.to_string())
}

/// HTML's character references, undone.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let name = &rest[1..end];
        let decoded = match name {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .or_else(|| name.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

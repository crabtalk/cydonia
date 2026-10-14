//! The Y Combinator directory: companies from yc-oss, founders from each
//! company's page on www.ycombinator.com, kept in the project's `companies`
//! and `founders` tables and refreshed one batch at a time.

pub mod scrape;
pub mod store;

use cydonia_guest::{
    App, fetch,
    node::*,
    tables,
    wire::{Align, ButtonStyle, Response, Rows, Space, Surface, Table, TextStyle, Tone},
};
use scrape::{Batch, Company, Founder, YC_OSS};
use std::ops::Range;
use store::{COMPANIES, COMPANY_COLUMNS, FOUNDER_COLUMNS, FOUNDERS, key, literal};

/// How many company pages are fetched at once.
const PAGES_AT_ONCE: usize = 4;
/// The height of a company's row in the list.
const ROW: f32 = 56.;

#[derive(Default)]
struct Yc {
    batches: Vec<Batch>,
    /// The batch listed, by slug.
    batch: Option<String>,
    /// The listed batch's companies, as the table holds them.
    companies: Vec<Company>,
    /// Where in `companies` each listed row is, after the search.
    shown: Vec<usize>,
    search: String,
    /// The company open on the right, by slug, and its founders.
    open: Option<String>,
    founders: Vec<Founder>,
    /// Whether both tables are there to read and write.
    ready: bool,
    refresh: Option<Refresh>,
    /// What the last refresh came to.
    report: Option<String>,
    /// The pages the last refresh could not read, by company name.
    failed: Vec<String>,
    error: Option<String>,
}

/// A batch being refreshed: its companies, and the founders read so far.
struct Refresh {
    batch: Batch,
    companies: Vec<Company>,
    founders: Vec<Founder>,
    /// The next company whose page is fetched.
    next: usize,
    /// How many pages have answered.
    read: usize,
    failed: Vec<String>,
}

#[derive(Clone)]
enum Msg {
    Tables(Result<Vec<Table>, String>),
    Created(Result<Table, String>),
    Meta(Result<Response, String>),
    Batch(String),
    Search(String),
    Loaded(Result<Rows, String>),
    Open(String),
    Founders(Result<Rows, String>),
    Refresh,
    Listing(Result<Response, String>),
    Page(usize, Result<Response, String>),
    Kept(Stage, Result<(), String>),
}

/// Writing a refreshed batch: its old rows go, then its new ones land.
#[derive(Clone, Copy)]
enum Stage {
    CompaniesCleared,
    CompaniesKept,
    FoundersCleared,
    FoundersKept,
}

impl App for Yc {
    type Msg = Msg;

    fn init() -> Self {
        tables::list(Msg::Tables);
        fetch::get(format!("{YC_OSS}/meta.json"), Msg::Meta);
        Yc::default()
    }

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Tables(Ok(tables)) => {
                let held = |name: &str| tables.iter().any(|table| table.key == key(name));
                match (held(COMPANIES), held(FOUNDERS)) {
                    (false, _) => tables::create(COMPANIES, COMPANY_COLUMNS, Msg::Created),
                    (_, false) => tables::create(FOUNDERS, FOUNDER_COLUMNS, Msg::Created),
                    (true, true) => {
                        self.ready = true;
                        self.load();
                    }
                }
            }
            Msg::Created(Ok(_)) => tables::list(Msg::Tables),
            Msg::Meta(answer) => match answer
                .and_then(|meta| ok(&meta))
                .and_then(|meta| Batch::list(&meta))
            {
                Ok(batches) => {
                    self.batch = batches
                        .iter()
                        .find(|batch| batch.count > 0)
                        .map(|batch| batch.slug.clone());
                    self.batches = batches;
                    self.load();
                }
                Err(error) => self.error = Some(format!("The batch list: {error}")),
            },
            Msg::Batch(slug) => {
                self.batch = Some(slug);
                self.open = None;
                self.founders.clear();
                self.load();
            }
            Msg::Search(text) => {
                self.search = text;
                self.filter();
            }
            Msg::Loaded(Ok(rows)) => {
                self.companies = rows.rows.iter().map(|row| Company::from_row(row)).collect();
                self.companies
                    .sort_by_key(|company| company.name.to_lowercase());
                self.filter();
            }
            Msg::Open(slug) => {
                tables::query(
                    store::query(
                        &FOUNDER_COLUMNS,
                        FOUNDERS,
                        &format!("company = {}", literal(&slug)),
                    ),
                    Msg::Founders,
                );
                self.open = Some(slug);
                self.founders.clear();
            }
            Msg::Founders(Ok(rows)) => {
                self.founders = rows.rows.iter().map(|row| Founder::from_row(row)).collect();
            }
            Msg::Refresh => self.refresh(),
            Msg::Listing(answer) => self.listed(answer),
            Msg::Page(ix, answer) => self.read_page(ix, answer),
            Msg::Kept(stage, Ok(())) => self.keep(stage),
            Msg::Tables(Err(error))
            | Msg::Created(Err(error))
            | Msg::Loaded(Err(error))
            | Msg::Founders(Err(error))
            | Msg::Kept(_, Err(error)) => {
                self.refresh = None;
                self.error = Some(error);
            }
        }
    }

    fn view(&self) -> Node<Msg> {
        row([self.sidebar().into(), self.detail().into()]).into()
    }

    fn rows(&self, list: &str, range: Range<u32>) -> Vec<Node<Msg>> {
        if list != "companies" {
            return Vec::new();
        }
        range
            .filter_map(|ix| self.companies.get(*self.shown.get(ix as usize)?))
            .map(|company| self.company_row(company))
            .collect()
    }
}

impl Yc {
    /// Read the listed batch's companies out of the table.
    fn load(&self) {
        if let (true, Some(batch)) = (self.ready, &self.batch) {
            tables::query(
                store::query(
                    &COMPANY_COLUMNS,
                    COMPANIES,
                    &format!("batch = {}", literal(batch)),
                ),
                Msg::Loaded,
            );
        }
    }

    fn filter(&mut self) {
        let search = self.search.trim().to_lowercase();
        self.shown = self
            .companies
            .iter()
            .enumerate()
            .filter(|(_, company)| {
                search.is_empty()
                    || company.name.to_lowercase().contains(&search)
                    || company.one_liner.to_lowercase().contains(&search)
            })
            .map(|(ix, _)| ix)
            .collect();
    }

    fn listed_batch(&self) -> Option<&Batch> {
        let slug = self.batch.as_ref()?;
        self.batches.iter().find(|batch| &batch.slug == slug)
    }

    fn refresh(&mut self) {
        let Some(batch) = self.listed_batch().cloned() else {
            return;
        };
        fetch::get(batch.url(), Msg::Listing);
        self.error = None;
        self.report = None;
        self.failed.clear();
        self.refresh = Some(Refresh {
            batch,
            companies: Vec::new(),
            founders: Vec::new(),
            next: 0,
            read: 0,
            failed: Vec::new(),
        });
    }

    fn listed(&mut self, answer: Result<Response, String>) {
        let Some(refresh) = &mut self.refresh else {
            return;
        };
        match answer
            .and_then(|listing| ok(&listing))
            .and_then(|listing| Company::list(&refresh.batch.slug, &listing))
        {
            Ok(companies) => {
                refresh.companies = companies;
                for _ in 0..PAGES_AT_ONCE {
                    self.fetch_page();
                }
                self.finish_if_read();
            }
            Err(error) => {
                self.error = Some(format!("{}: {error}", refresh.batch.name));
                self.refresh = None;
            }
        }
    }

    /// Fetch the next company's page, if there is one left.
    fn fetch_page(&mut self) {
        let Some(refresh) = &mut self.refresh else {
            return;
        };
        let ix = refresh.next;
        let Some(company) = refresh.companies.get(ix) else {
            return;
        };
        refresh.next += 1;
        fetch::get(company.url.clone(), move |page| Msg::Page(ix, page));
    }

    fn read_page(&mut self, ix: usize, answer: Result<Response, String>) {
        let Some(refresh) = &mut self.refresh else {
            return;
        };
        refresh.read += 1;
        if let Some(company) = refresh.companies.get_mut(ix) {
            match answer
                .and_then(|page| ok(&page))
                .and_then(|page| company.read_page(&page))
            {
                Ok(founders) => refresh.founders.extend(founders),
                Err(error) => refresh.failed.push(format!("{} ({error})", company.name)),
            }
        }
        self.fetch_page();
        self.finish_if_read();
    }

    /// Once every page has answered, replace the batch's rows.
    fn finish_if_read(&mut self) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        if refresh.read == refresh.companies.len() {
            tables::delete(
                key(COMPANIES),
                "batch",
                refresh.batch.slug.clone(),
                |done| Msg::Kept(Stage::CompaniesCleared, done),
            );
        }
    }

    fn keep(&mut self, stage: Stage) {
        let Some(refresh) = &self.refresh else {
            return;
        };
        let slug = refresh.batch.slug.clone();
        match stage {
            Stage::CompaniesCleared => {
                let rows = refresh.companies.iter().map(Company::record).collect();
                tables::insert(key(COMPANIES), rows, |done| {
                    Msg::Kept(Stage::CompaniesKept, done.map(|_| ()))
                });
            }
            Stage::CompaniesKept => tables::delete(key(FOUNDERS), "batch", slug, |done| {
                Msg::Kept(Stage::FoundersCleared, done)
            }),
            Stage::FoundersCleared => {
                let rows = refresh.founders.iter().map(Founder::record).collect();
                tables::insert(key(FOUNDERS), rows, |done| {
                    Msg::Kept(Stage::FoundersKept, done.map(|_| ()))
                });
            }
            Stage::FoundersKept => {
                let Some(refresh) = self.refresh.take() else {
                    return;
                };
                self.report = Some(format!(
                    "{}: {} and {} kept",
                    refresh.batch.name,
                    count(refresh.companies.len(), "company", "companies"),
                    count(refresh.founders.len(), "founder", "founders")
                ));
                self.failed = refresh.failed;
                self.load();
                if let Some(open) = self.open.clone() {
                    self.update(Msg::Open(open));
                }
            }
        }
    }

    fn sidebar(&self) -> Layout<Msg> {
        let options = self.batches.iter().map(|batch| {
            (
                batch.slug.clone(),
                format!("{} ({})", batch.name, batch.count),
            )
        });
        let refresh = match &self.refresh {
            Some(_) => button("Refreshing…"),
            None => button("Refresh")
                .style(ButtonStyle::Prominent)
                .on_click(Msg::Refresh),
        };
        let status = match &self.refresh {
            Some(refresh) => Some(format!(
                "{}: {} of {} pages read",
                refresh.batch.name,
                refresh.read,
                refresh.companies.len()
            )),
            None => self.report.clone(),
        };
        column([
            input("search")
                .placeholder("Search companies")
                .on_input(Msg::Search)
                .into(),
            row([
                column([select("batch", options)
                    .placeholder("Batch")
                    .selected(self.batch.clone())
                    .on_select(Msg::Batch)
                    .into()])
                .grow()
                .into(),
                refresh.into(),
            ])
            .gap(Space::Small)
            .align(Align::Center)
            .into(),
        ])
        .children(status.map(|status| {
            text(status)
                .style(TextStyle::Caption)
                .tone(Tone::Muted)
                .into()
        }))
        .children((!self.failed.is_empty()).then(|| {
            text(format!(
                "{} failed: {}",
                count(self.failed.len(), "page", "pages"),
                self.failed.join(", ")
            ))
            .style(TextStyle::Caption)
            .tone(Tone::Danger)
            .lines(4)
            .into()
        }))
        .children(self.error.iter().map(|error| {
            text(error.clone())
                .style(TextStyle::Caption)
                .tone(Tone::Danger)
                .into()
        }))
        .children(
            (self.companies.is_empty() && self.refresh.is_none()).then(|| {
                text("No companies kept for this batch yet. Refresh fetches them.")
                    .style(TextStyle::Callout)
                    .tone(Tone::Muted)
                    .into()
            }),
        )
        .child(
            column([list("companies", self.shown.len() as u32, ROW).into()])
                .grow()
                .surface(Surface::Card),
        )
        .width(360.)
        .gap(Space::Medium)
        .padding(Space::Large)
    }

    fn company_row(&self, company: &Company) -> Node<Msg> {
        let open = self.open.as_deref() == Some(company.slug.as_str());
        let row = row([
            logo(&company.logo, 32.),
            column([
                button(company.name.clone())
                    .key(company.slug.clone())
                    .on_click(Msg::Open(company.slug.clone()))
                    .into(),
                text(company.one_liner.clone())
                    .style(TextStyle::Caption)
                    .tone(Tone::Muted)
                    .lines(1)
                    .into(),
            ])
            .grow()
            .into(),
        ])
        .key(company.slug.clone())
        .gap(Space::Small)
        .padding(Space::Small)
        .align(Align::Center);
        match open {
            true => row.surface(Surface::Raised).into(),
            false => row.into(),
        }
    }

    fn detail(&self) -> Layout<Msg> {
        let open = self
            .open
            .as_ref()
            .and_then(|slug| self.companies.iter().find(|company| &company.slug == slug));
        let Some(company) = open else {
            return column([text("Pick a company.").tone(Tone::Muted).into()])
                .grow()
                .padding(Space::Large);
        };
        let facts: Vec<String> = [
            company.batch_name.clone(),
            [company.city.as_str(), company.country.as_str()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
            company
                .year_founded
                .map(|year| format!("founded {year}"))
                .unwrap_or_default(),
            company
                .team_size
                .map(|size| format!("{size} people"))
                .unwrap_or_default(),
            company.status.clone(),
            company.industry.clone(),
            match company.partner.is_empty() {
                true => String::new(),
                false => format!("partner {}", company.partner),
            },
        ]
        .into_iter()
        .filter(|fact| !fact.is_empty())
        .collect();
        let mut links = Vec::new();
        if !company.website.is_empty() {
            links.push(link(company.website.clone(), company.website.clone()).into());
        }
        if !company.url.is_empty() {
            links.push(link("YC page", company.url.clone()).into());
        }
        column([
            row([
                logo(&company.logo, 64.),
                column([
                    text(company.name.clone()).style(TextStyle::Title).into(),
                    text(company.one_liner.clone()).tone(Tone::Muted).into(),
                ])
                .grow()
                .gap(Space::Small)
                .into(),
            ])
            .gap(Space::Medium)
            .align(Align::Center)
            .into(),
            text(facts.join(" · "))
                .style(TextStyle::Callout)
                .tone(Tone::Muted)
                .into(),
            row(links).gap(Space::Medium).into(),
            text("Founders").style(TextStyle::Headline).into(),
        ])
        .children(self.founders.iter().map(founder_card))
        .children(self.founders.is_empty().then(|| {
            text("No founders kept for this company. Refreshing its batch reads its page.")
                .style(TextStyle::Callout)
                .tone(Tone::Muted)
                .into()
        }))
        .key(company.slug.clone())
        .grow()
        .scroll()
        .gap(Space::Medium)
        .padding(Space::Large)
    }
}

fn founder_card(founder: &Founder) -> Node<Msg> {
    let mut links = Vec::new();
    if !founder.linkedin.is_empty() {
        links.push(link("LinkedIn", founder.linkedin.clone()).into());
    }
    if !founder.x.is_empty() {
        links.push(link("X", founder.x.clone()).into());
    }
    let avatar = match founder.avatar.is_empty() {
        true => column([]).width(48.).height(48.).into(),
        false => image(founder.avatar.clone(), 48., 48.).round().into(),
    };
    row([
        avatar,
        column([
            text(founder.name.clone()).style(TextStyle::Headline).into(),
            text(founder.title.clone())
                .style(TextStyle::Callout)
                .tone(Tone::Muted)
                .into(),
            text(founder.bio.clone()).into(),
            row(links).gap(Space::Medium).into(),
        ])
        .grow()
        .gap(Space::Small)
        .into(),
    ])
    .gap(Space::Medium)
    .padding(Space::Medium)
    .surface(Surface::Card)
    .into()
}

/// A company's logo, or the room for one.
fn logo(url: &str, size: f32) -> Node<Msg> {
    match url.is_empty() {
        true => column([]).width(size).height(size).into(),
        false => image(url.to_owned(), size, size).into(),
    }
}

/// `n` of something, as a count reads: `1 page`, `3 pages`.
fn count(n: usize, one: &str, many: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        n => format!("{n} {many}"),
    }
}

/// A response's body as text, when it is a 2xx.
fn ok(response: &Response) -> Result<String, String> {
    match response.status {
        200..=299 => Ok(response.text()),
        status => Err(format!("HTTP {status}")),
    }
}

cydonia_guest::app!(
    Yc,
    name = "Y Combinator",
    calls = [tables],
    hosts = [
        "yc-oss.github.io",
        "www.ycombinator.com",
        "bookface-images.s3.amazonaws.com"
    ]
);

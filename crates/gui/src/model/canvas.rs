use crate::model::store::Store;
use artifact::{
    article::Article,
    canvas::{Canvas as Record, Properties},
    kind::Kind,
    project::Project as _,
    reference::Reference,
};
use bezel::gpui::{AppContext as _, Context, Entity, Subscription};
use canvas::{Change, Document, DocumentChange, DocumentEvent, model};

pub struct Canvas {
    pub record: Record,
    pub document: Entity<Document>,
    pub error: Option<String>,
    store: Store,
    _changed: Subscription,
}

impl Canvas {
    pub fn new(record: Record, store: Store, cx: &mut Context<Self>) -> Self {
        let document = cx.new(|_| Document::new(record.document.clone()));
        let changed = cx.subscribe(&document, |this, _, event: &DocumentEvent, cx| {
            if matches!(event.change, DocumentChange::Edited(_)) {
                this.save(cx);
            }
        });
        Self {
            record,
            document,
            error: None,
            store,
            _changed: changed,
        }
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let current = (**self.document.read(cx)).clone();
        if self.record.document == current && self.error.is_none() {
            return;
        }
        self.record.document = current;
        self.error = self
            .store
            .save_canvas(&mut self.record)
            .err()
            .map(|error| error.to_string());
        cx.notify();
    }

    pub fn convert_to_article(
        &mut self,
        id: &str,
        project: &str,
        cx: &mut Context<Self>,
    ) -> Result<Article, String> {
        let mut node = self
            .document
            .read(cx)
            .node(id)
            .cloned()
            .ok_or("card no longer exists")?;
        if node.kind != model::TEXT {
            return Err("only a text card can become an article".into());
        }
        let article = self
            .store
            .create_article(node.text.as_deref().unwrap_or_default())
            .map_err(|error| error.to_string())?;
        let number = self
            .store
            .number(Kind::Article, &article.id)
            .map_err(|error| error.to_string())?;
        node.kind = model::LINK.into();
        node.text = None;
        node.url = Some(Reference::entry(Some(project), number, None).link());
        let mut record = self.record.clone();
        record.document = (**self.document.read(cx)).clone();
        *record.document.node_mut(id).expect("node was found") = node.clone();
        if let Err(error) = self.store.save_canvas(&mut record) {
            let _ = self.store.remove_article(&article.id);
            return Err(error.to_string());
        }
        self.record = record;
        self.document.update(cx, |document, cx| {
            document.edit(vec![Change::UpdateNode { node }], cx)
        });
        self.error = None;
        cx.notify();
        Ok(article)
    }

    pub fn rename(&mut self, title: String, cx: &mut Context<Self>) -> bool {
        self.edit_metadata(|properties| properties.title = title, cx)
    }

    pub fn archive(&mut self, archived: bool, cx: &mut Context<Self>) -> bool {
        self.edit_metadata(|properties| properties.archived = archived, cx)
    }

    pub fn set_labels(&mut self, labels: Vec<String>, cx: &mut Context<Self>) -> bool {
        self.edit_metadata(|properties| properties.labels = labels, cx)
    }

    fn edit_metadata(
        &mut self,
        edit: impl FnOnce(&mut Properties),
        cx: &mut Context<Self>,
    ) -> bool {
        let mut next = self.record.clone();
        next.document = (**self.document.read(cx)).clone();
        edit(&mut next.properties);
        let saved = match self.store.save_canvas(&mut next) {
            Ok(()) => {
                self.record = next;
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        };
        cx.notify();
        saved
    }

    pub fn adopt(&mut self, record: Record, cx: &mut Context<Self>) {
        if self.record.version == record.version || self.error.is_some() {
            return;
        }
        self.document.update(cx, |document, cx| {
            document.reload(record.document.clone(), cx)
        });
        self.record = record;
        cx.notify();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(record) = self.store.canvas(&self.record.id) {
            self.document.update(cx, |document, cx| {
                document.reload(record.document.clone(), cx)
            });
            self.record = record;
            self.error = None;
            cx.notify();
        }
    }

    pub fn keep(&mut self, cx: &mut Context<Self>) {
        if let Some(record) = self.store.canvas(&self.record.id) {
            self.record.version = record.version;
            self.save(cx);
        }
    }
}

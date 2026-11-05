use crate::model::store::Store;
use artifact::{
    canvas::{Canvas as Record, Properties},
    project::Project as _,
};
use bezel::gpui::{AppContext as _, Context, Entity, Subscription};
use canvas::{Document, DocumentChange, DocumentEvent};

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
        self.record.document = (**self.document.read(cx)).clone();
        self.error = self
            .store
            .save_canvas(&mut self.record)
            .err()
            .map(|error| error.to_string());
        cx.notify();
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

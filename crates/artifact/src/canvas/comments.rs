use crate::{
    canvas::{Canvas, web},
    comment::Thread,
    stamp,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub url: String,
    pub selector: String,
    pub quote: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub node: String,
    pub anchor: Anchor,
    #[serde(flatten)]
    pub thread: Thread,
}

impl Canvas {
    pub fn web_comment(
        &mut self,
        node: String,
        anchor: Anchor,
        note: String,
    ) -> Result<u64, String> {
        if !self.has_web_frame(&node) {
            return Err(format!("no web frame {node} on this canvas"));
        }
        let id = Thread::fresh_id(
            stamp::now() as u64,
            self.comments.iter().map(|comment| comment.thread.id),
        );
        self.comments.push(Comment {
            node,
            anchor,
            thread: Thread::new(id, note, None),
        });
        Ok(id)
    }

    pub fn has_web_frame(&self, node: &str) -> bool {
        self.document
            .node(node)
            .is_some_and(|node| node.kind == web::KIND)
    }
}

use crate::{
    model::{settings::Settings, state::State, workspace::Workspace},
    view::canvas::image::Images,
};
use bezel::gpui::{
    AppContext as _, Context, Entity, EntityId, Render, TestAppContext, VisualTestContext, Window,
    div,
};
use canvas::model::Node;
use std::{cell::RefCell, path::Path, rc::Rc};

struct Host {
    images: Images,
    seen: Rc<RefCell<Vec<EntityId>>>,
    _workspace: Entity<Workspace>,
}
impl Render for Host {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl bezel::gpui::IntoElement {
        let node = Node {
            id: "image".into(),
            file: Some("picture.png".into()),
            ..Default::default()
        };
        let mut seen = self.seen.borrow_mut();
        seen.clear();
        for scope in ["body", "opener"] {
            let image = window
                .with_id(scope, |window| {
                    self.images.for_node(&node, Path::new("/tmp"), window, cx)
                })
                .unwrap();
            seen.push(image.entity_id());
        }
        div()
    }
}
#[gpui::test]
fn body_and_opener_share_image_state_across_view_scopes(cx: &mut TestAppContext) {
    let workspace = cx.new(|cx| Workspace::new(Settings::default(), State::default(), cx));
    cx.update(|cx| Workspace::install(&workspace, cx));
    let seen = Rc::new(RefCell::new(Vec::new()));
    let window = cx.add_window({
        let seen = seen.clone();
        move |_, _| Host {
            images: Images::default(),
            seen,
            _workspace: workspace,
        }
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], seen[1]);
}

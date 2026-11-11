use rouge_core::{BuildCx, Div, ElementId, PaintRegion, Rect, View, div, show};
use rouge_reactive::{Memo, Signal, memo, on_cleanup, provide_context, signal, use_context};
use std::{collections::BTreeMap, rc::Rc};

type Marks = BTreeMap<u64, (bool, Signal<PaintRegion>)>;

#[derive(Clone, Copy)]
pub struct IndicatorLayer {
    marks: Signal<Marks>,
    next: Signal<u64>,
}

impl IndicatorLayer {
    #[must_use]
    pub fn install() -> Self {
        let layer = Self { marks: signal(BTreeMap::new()), next: signal(0) };
        provide_context(layer);
        layer
    }

    #[must_use]
    pub fn shown(self) -> Memo<Vec<Rect>> {
        memo(move || {
            self.marks.with(|marks| {
                marks
                    .iter()
                    .filter_map(|(&id, &(cover, _))| (!cover).then(|| visible_region(marks, id)).flatten())
                    .collect()
            })
        })
    }

    fn register(self, cover: bool, region: Signal<PaintRegion>) -> u64 {
        let id = self.next.get_untracked();
        self.next.set(id + 1);
        self.marks.update(|marks| {
            marks.insert(id, (cover, region));
        });
        on_cleanup(move || {
            if self.marks.is_alive() {
                self.marks.update(|marks| {
                    marks.remove(&id);
                });
            }
        });
        id
    }
}

fn visible_region(marks: &Marks, id: u64) -> Option<Rect> {
    let (_, region) = marks.get(&id)?;
    let region = region.get();
    if region.bounds.is_empty() {
        return None;
    }
    let covered = marks.values().any(|&(cover, other)| {
        if !cover {
            return false;
        }
        let other = other.get();
        other.order > region.order && !other.bounds.intersect(&region.bounds).is_empty()
    });
    (!covered).then_some(region.bounds)
}

type Content = Box<dyn FnOnce(Div, Option<Memo<bool>>) -> Div>;

pub struct IndicatorMark {
    root: Div,
    cover: bool,
    content: Option<Content>,
}

impl IndicatorMark {
    pub fn occluder(id: impl Into<String>) -> Self {
        Self { root: div().test_id(id).pass_through(), cover: true, content: None }
    }

    pub fn slot<T: View>(id: impl Into<String>, content: impl Fn() -> T + 'static) -> Self {
        Self::tracked(id, move |root, visible| {
            let content = Rc::new(content);
            if let Some(visible) = visible {
                root.child(show(move || visible.get(), move || content()))
            } else {
                root.child(content())
            }
        })
    }

    pub(crate) fn tracked(
        id: impl Into<String>,
        content: impl FnOnce(Div, Option<Memo<bool>>) -> Div + 'static,
    ) -> Self {
        Self { root: div().test_id(id), cover: false, content: Some(Box::new(content)) }
    }
}
rouge_core::styled!(IndicatorMark => root);

impl View for IndicatorMark {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let mut root = self.root;
        let visible = use_context::<IndicatorLayer>().map(|layer| {
            let region = signal(PaintRegion::default());
            let id = layer.register(self.cover, region);
            (region, memo(move || layer.marks.with(|marks| visible_region(marks, id).is_some())))
        });
        if let Some((region, _)) = visible {
            root = root.track_paint_region(region);
        }
        if let Some(content) = self.content {
            root = content(root, visible.map(|(_, visible)| visible));
        }
        root.build(cx)
    }
}

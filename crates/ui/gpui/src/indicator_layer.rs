use crate::waveform::{animates, paint_waveform, phase, tick};
use gpui::{
    App, Bounds, ContentMask, Context, ElementId, EntityId, Global, GlobalElementId, Hsla, MouseMoveEvent, Pixels,
    Render, SharedString, Subscription, Task, Window, WindowId, canvas, div, prelude::*,
};
use marker::{Marker, OccluderMark, SlotMark};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    mem,
    rc::{Rc, Weak},
};
use web_time::Instant;

pub struct IndicatorLayer {
    state: Rc<RefCell<LayerState>>,
    _visibility: Subscription,
}

impl IndicatorLayer {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let layer = cx.entity_id();
        let state = Rc::new(RefCell::new(LayerState::new(layer, cx.background_executor().now())));
        let layers = &mut cx.default_global::<IndicatorLayers>().0;
        layers.retain(|_, state| state.strong_count() > 0);
        layers.insert(window.window_handle().window_id(), Rc::downgrade(&state));
        let visibility = window.observe_window_visibility(move |visibility, _, cx| {
            if visibility.is_visible() {
                cx.notify(layer);
            }
        });
        Self { state, _visibility: visibility }
    }

    #[must_use]
    pub fn shown(&self) -> Vec<Bounds<Pixels>> {
        self.state.borrow().shown.clone()
    }
}

impl Render for IndicatorLayer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.state.borrow_mut().frame += 1;
        let state = Rc::clone(&self.state);
        div()
            .absolute()
            .inset_0()
            .child(canvas(|_, _, _| {}, move |_, (), window, cx| paint_layer(&state, window, cx)).size_full())
    }
}

pub type IndicatorOccluder = Marker<OccluderMark>;

impl IndicatorOccluder {
    #[must_use]
    pub fn new(id: impl Into<ElementId>) -> Self {
        Marker::with(id.into(), OccluderMark)
    }
}

pub(crate) type IndicatorSlot = Marker<SlotMark>;

impl IndicatorSlot {
    pub(crate) fn new(id: SharedString, color: Hsla, state: Rc<RefCell<LayerState>>) -> Self {
        Marker::with(ElementId::Name(id), SlotMark { color, state })
    }
}

pub(crate) struct LayerState {
    layer: EntityId,
    epoch: Instant,
    frame: u64,
    painted_frame: Option<u64>,
    requested: bool,
    tick: Option<Task<()>>,
    in_place_pending: HashSet<EntityId>,
    slots: HashMap<GlobalElementId, Slot>,
    occluders: HashMap<GlobalElementId, Region>,
    shown: Vec<Bounds<Pixels>>,
}

struct Slot {
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    color: Hsla,
    alive: Weak<()>,
}

struct Region {
    bounds: Bounds<Pixels>,
    alive: Weak<()>,
}

#[derive(Default)]
struct IndicatorLayers(HashMap<WindowId, Weak<RefCell<LayerState>>>);

impl Global for IndicatorLayers {}

pub(crate) fn layer_state(window: &Window, cx: &App) -> Option<Rc<RefCell<LayerState>>> {
    cx.try_global::<IndicatorLayers>()?.0.get(&window.window_handle().window_id())?.upgrade()
}

impl LayerState {
    fn new(layer: EntityId, epoch: Instant) -> Self {
        Self {
            layer,
            epoch,
            frame: 0,
            painted_frame: None,
            requested: false,
            tick: None,
            in_place_pending: HashSet::new(),
            slots: HashMap::new(),
            occluders: HashMap::new(),
            shown: Vec::new(),
        }
    }

    fn phase(&self, cx: &App) -> f32 {
        phase(cx.background_executor().now().saturating_duration_since(self.epoch))
    }

    const fn painted_this_frame(&self) -> bool {
        matches!(self.painted_frame, Some(frame) if frame == self.frame)
    }

    const fn request_frame(&mut self) -> EntityId {
        self.requested = true;
        self.layer
    }
}

fn paint_layer(state: &Rc<RefCell<LayerState>>, window: &mut Window, cx: &mut App) {
    let mut layer = state.borrow_mut();
    let layer = &mut *layer;
    layer.painted_frame = Some(layer.frame);
    layer.slots.retain(|_, slot| slot.alive.strong_count() > 0);
    layer.occluders.retain(|_, region| region.alive.strong_count() > 0);
    let animate = animates(window, cx);
    let phase = if animate { layer.phase(cx) } else { 0.0 };
    layer.shown.clear();
    for slot in layer.slots.values() {
        let occluded = layer.occluders.values().any(|region| region.bounds.intersects(&slot.bounds));
        if occluded || slot.bounds.intersect(&slot.mask.bounds).is_empty() {
            continue;
        }
        window.with_content_mask(Some(slot.mask), |window| {
            paint_waveform(slot.bounds, phase, slot.color, window);
        });
        layer.shown.push(slot.bounds);
    }
    let requested = mem::take(&mut layer.requested);
    if layer.slots.is_empty() || !animate {
        layer.tick = None;
    } else if layer.tick.is_none() {
        layer.tick = Some(notify_after_tick(state, window, cx, |layer| {
            layer.tick = None;
            layer.request_frame()
        }));
    }
    if !requested && !layer.slots.is_empty() {
        let state = Rc::downgrade(state);
        window.on_next_frame(move |_, cx| {
            if let Some(state) = state.upgrade() {
                let layer = state.borrow_mut().request_frame();
                cx.notify(layer);
            }
        });
    }
}

fn paint_in_place(
    state: &Rc<RefCell<LayerState>>,
    bounds: Bounds<Pixels>,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let mut layer = state.borrow_mut();
    let animate = animates(window, cx);
    paint_waveform(bounds, if animate { layer.phase(cx) } else { 0.0 }, color, window);
    let host = window.current_view();
    if animate && layer.in_place_pending.insert(host) {
        notify_after_tick(state, window, cx, move |layer| {
            layer.in_place_pending.remove(&host);
            host
        })
        .detach();
    }
}

fn notify_after_tick(
    state: &Rc<RefCell<LayerState>>,
    window: &Window,
    cx: &App,
    fire: impl FnOnce(&mut LayerState) -> EntityId + 'static,
) -> Task<()> {
    let state = Rc::downgrade(state);
    window.spawn(cx, async move |cx| {
        cx.background_executor().timer(tick()).await;
        let Some(state) = state.upgrade() else {
            return;
        };
        let entity = fire(&mut state.borrow_mut());
        cx.update(|_, cx| cx.notify(entity)).ok();
    })
}

fn keep_alive(window: &mut Window) -> Weak<()> {
    let alive = Rc::new(());
    let weak = Rc::downgrade(&alive);
    window.on_mouse_event(move |_: &MouseMoveEvent, _, _, _| {
        let _ = &alive;
    });
    weak
}

mod marker {
    use super::{LayerState, Region, Slot, keep_alive, layer_state, paint_in_place};
    use gpui::{
        App, Bounds, Element, ElementId, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels,
        Refineable, Style, StyleRefinement, Styled, Window,
    };
    use std::{cell::RefCell, panic::Location, rc::Rc};

    pub trait Mark: 'static {
        fn paint(&self, id: &GlobalElementId, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App);
    }

    pub struct Marker<T> {
        id: ElementId,
        style: StyleRefinement,
        mark: T,
    }

    impl<T> Marker<T> {
        pub(super) fn with(id: ElementId, mark: T) -> Self {
            Self { id, style: StyleRefinement::default(), mark }
        }
    }

    pub struct OccluderMark;

    impl Mark for OccluderMark {
        fn paint(&self, id: &GlobalElementId, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
            let Some(state) = layer_state(window, cx) else {
                return;
            };
            let region = Region { bounds: bounds.intersect(&window.content_mask().bounds), alive: keep_alive(window) };
            state.borrow_mut().occluders.insert(id.clone(), region);
        }
    }

    pub struct SlotMark {
        pub(super) color: Hsla,
        pub(super) state: Rc<RefCell<LayerState>>,
    }

    impl Mark for SlotMark {
        fn paint(&self, id: &GlobalElementId, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
            if self.state.borrow().painted_this_frame() {
                paint_in_place(&self.state, bounds, self.color, window, cx);
                return;
            }
            let slot = Slot { bounds, mask: window.content_mask(), color: self.color, alive: keep_alive(window) };
            self.state.borrow_mut().slots.insert(id.clone(), slot);
        }
    }

    impl<T: Mark> IntoElement for Marker<T> {
        type Element = Self;

        fn into_element(self) -> Self {
            self
        }
    }

    impl<T> Styled for Marker<T> {
        fn style(&mut self) -> &mut StyleRefinement {
            &mut self.style
        }
    }

    impl<T: Mark> Element for Marker<T> {
        type RequestLayoutState = ();
        type PrepaintState = ();

        fn id(&self) -> Option<ElementId> {
            Some(self.id.clone())
        }

        fn source_location(&self) -> Option<&'static Location<'static>> {
            None
        }

        fn request_layout(
            &mut self,
            _id: Option<&GlobalElementId>,
            _inspector_id: Option<&InspectorElementId>,
            window: &mut Window,
            cx: &mut App,
        ) -> (LayoutId, ()) {
            let mut style = Style::default();
            style.refine(&self.style);
            (window.request_layout(style, [], cx), ())
        }

        fn prepaint(
            &mut self,
            _id: Option<&GlobalElementId>,
            _inspector_id: Option<&InspectorElementId>,
            _bounds: Bounds<Pixels>,
            _request_layout: &mut (),
            _window: &mut Window,
            _cx: &mut App,
        ) {
        }

        fn paint(
            &mut self,
            id: Option<&GlobalElementId>,
            _inspector_id: Option<&InspectorElementId>,
            bounds: Bounds<Pixels>,
            _request_layout: &mut (),
            _prepaint: &mut (),
            window: &mut Window,
            cx: &mut App,
        ) {
            if let Some(id) = id {
                self.mark.paint(id, bounds, window, cx);
            }
        }
    }
}

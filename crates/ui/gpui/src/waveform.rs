use crate::indicator_layer::{IndicatorMark, layer_state};
use crate::tokens::{ICON_SIZE, rem};
use gpui::{
    Animation, AnimationExt, App, Bounds, Hsla, IntoElement, Pixels, RenderOnce, SharedString, Window, canvas, div,
    fill, point, prelude::*, size,
};
use std::f32::consts::TAU;
use std::time::Duration;

#[derive(IntoElement)]
pub struct Waveform {
    id: SharedString,
    color: Hsla,
    size: f32,
}

impl Waveform {
    #[must_use]
    pub fn new(id: impl Into<SharedString>, color: Hsla) -> Self {
        Self { id: id.into(), color, size: ICON_SIZE }
    }

    #[must_use]
    pub const fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Waveform {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self { id, color, size } = self;
        let frame = div().flex_none().size(rem(size));
        if let Some(state) = layer_state(window, cx) {
            return frame.child(IndicatorMark::slot(id, color, state).size_full()).into_any_element();
        }
        if !animates(window, cx) {
            return frame.child(bars(0.0, color)).into_any_element();
        }
        let animation = Animation::new(PERIOD).repeat_synced().with_max_fps(MAX_FPS);
        frame.with_animation(id, animation, move |frame, phase| frame.child(bars(phase, color))).into_any_element()
    }
}

fn bars(phase: f32, color: Hsla) -> impl IntoElement {
    canvas(|_, _, _| {}, move |bounds, (), window, _| paint_waveform(bounds, phase, color, window)).size_full()
}

const PERIOD: Duration = Duration::from_secs(3);
const MAX_FPS: f32 = 30.0;

pub(crate) fn animates(window: &Window, cx: &App) -> bool {
    window.is_visible() && !cx.reduce_motion()
}

pub(crate) fn phase(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() % PERIOD.as_secs_f32()) / PERIOD.as_secs_f32()
}

pub(crate) fn tick() -> Duration {
    Duration::from_secs_f32(1.0 / MAX_FPS)
}

struct Bar {
    peak: f32,
    swells: f32,
    phase: f32,
}

const BARS: [Bar; 5] = [
    Bar { peak: 0.45, swells: 3.0, phase: 0.0 },
    Bar { peak: 0.75, swells: 2.0, phase: 0.35 },
    Bar { peak: 1.0, swells: 3.0, phase: 0.2 },
    Bar { peak: 0.75, swells: 4.0, phase: 0.6 },
    Bar { peak: 0.45, swells: 3.0, phase: 0.3 },
];

const REST: f32 = 0.18;
const BAR_WIDTH: f32 = 0.14;
const BAR_GAP: f32 = 0.075;

#[expect(clippy::cast_precision_loss, reason = "five bars")]
pub(crate) fn paint_waveform(bounds: Bounds<Pixels>, phase: f32, color: Hsla, window: &mut Window) {
    let extent = bounds.size.width.min(bounds.size.height);
    let (width, gap) = (extent * BAR_WIDTH, extent * BAR_GAP);
    let span = width * BARS.len() as f32 + gap * (BARS.len() - 1) as f32;
    let center = bounds.center();
    let mut left = center.x - span / 2.0;
    for bar in &BARS {
        let swell = 0.5 - 0.5 * (TAU * (phase * bar.swells + bar.phase)).cos();
        let height = extent * (REST + (bar.peak - REST) * swell);
        let bar_bounds = Bounds::new(point(left, center.y - height / 2.0), size(width, height));
        window.paint_quad(fill(bar_bounds, color.opacity(0.5 + 0.5 * swell)).corner_radii(width / 2.0));
        left += width + gap;
    }
}

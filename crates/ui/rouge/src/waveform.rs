use crate::{
    indicator_layer::IndicatorMark,
    tokens::{ICON_SIZE, rem, scaled},
};
use rouge_core::{BuildCx, Color, ElementId, IntoProp, Keyframes, Prop, Styled, View, div};
use std::rc::Rc;

pub struct Waveform {
    id: String,
    color: Prop<Color>,
    size: f32,
    levels: Option<Prop<[f32; 5]>>,
}
impl Waveform {
    pub fn new<T>(id: impl Into<String>, color: impl IntoProp<Color, T>) -> Self {
        Self { id: id.into(), color: color.into_prop(), size: ICON_SIZE, levels: None }
    }
    #[must_use]
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    #[must_use]
    pub fn levels<T>(mut self, levels: impl IntoProp<[f32; 5], T>) -> Self {
        self.levels = Some(levels.into_prop());
        self
    }
}
impl View for Waveform {
    fn build(self, cx: &mut BuildCx<'_>) -> ElementId {
        let color = Rc::new(self.color);
        let levels = self.levels.map(Rc::new);
        let size = self.size;
        IndicatorMark::slot(self.id, move || {
            let color = color.clone();
            let levels = levels.clone();
            let bars = [0.0, 0.16, 0.32, 0.48, 0.64].into_iter().enumerate().map(move |(i, phase)| {
                let color = color.clone();
                let bar = div().w(rem(size / 9.0)).rounded(scaled(1.0)).bg(move || color.get());
                if let Some(levels) = &levels {
                    let levels = levels.clone();
                    bar.h(move || rem(size * (0.15 + 0.75 * levels.get()[i].clamp(0.0, 1.0))))
                } else {
                    bar.h(rem(size * 0.83)).animate(
                        Keyframes::new()
                            .at(0.0, |p| p.scale(0.25 + phase))
                            .at(0.5, |p| p.scale(1.0 - phase))
                            .at(1.0, |p| p.scale(0.25 + phase))
                            .duration(1.1)
                            .repeat(),
                    )
                }
            });
            div().size_full().flex().items_center().justify_center().gap(rem(size / 12.0)).children(bars)
        })
        .w(rem(size))
        .h(rem(size))
        .flex_none()
        .build(cx)
    }
}

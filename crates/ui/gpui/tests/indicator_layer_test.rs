use clankerdiff_gpui_ui::{
    prelude::{Backdrop, IndicatorLayer, UiTheme, Waveform},
    testing::{ViewHarness, advance_clock},
};
use clankerdiff_ui_core::ReviewTheme;
use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Entity, EventEmitter, IntoElement, Pixels, Render, StyleRefinement,
    TestAppContext, Window, WindowOptions, WindowVisibility, black, div, prelude::*, px,
};
use std::{error::Error, time::Duration};

#[gpui::test]
fn a_layered_waveform_animates_without_rerendering_its_cached_host(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        let before = host_renders(harness, cx);

        let frames = frames_over(harness, cx, Duration::from_secs(1));

        assert!((10..=30).contains(&frames), "drew {frames} frames in a second");
        assert_eq!(host_renders(harness, cx), before, "the host is reused from its cache");
        assert_eq!(shown(harness, cx), vec![frame(harness, cx)?]);
        Ok(())
    });
}

#[gpui::test]
fn the_layer_stops_drawing_a_waveform_its_host_no_longer_shows(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        update_host(harness, cx, |host| host.waveform = false);

        assert_eq!(shown(harness, cx), Vec::new());
        assert_eq!(frames_over(harness, cx, Duration::from_secs(1)), 0, "nothing is left to animate");
        Ok(())
    });
}

#[gpui::test]
fn a_host_dropped_from_the_window_takes_its_waveform_with_it(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        harness.update(cx, |stage, cx| {
            stage.host_shown = false;
            cx.notify();
        });
        harness.draw(cx);

        assert_eq!(shown(harness, cx), Vec::new());
        assert_eq!(frames_over(harness, cx, Duration::from_secs(1)), 0);
        Ok(())
    });
}

#[gpui::test]
fn a_backdrop_hides_the_waveforms_beneath_it(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        update_host(harness, cx, |host| host.occluded = true);
        assert_eq!(shown(harness, cx), Vec::new());

        update_host(harness, cx, |host| host.occluded = false);
        assert_eq!(shown(harness, cx), vec![frame(harness, cx)?]);
        Ok(())
    });
}

#[gpui::test]
fn a_waveform_painted_above_the_layer_animates_in_place(cx: &mut TestAppContext) {
    StageBuilder::default().host_above_layer().run(cx, |harness, cx| {
        let before = host_renders(harness, cx);

        frames_over(harness, cx, Duration::from_secs(1));

        let redraws = host_renders(harness, cx) - before;
        assert!((10..=30).contains(&redraws), "the host redrew {redraws} times in a second");
        assert_eq!(shown(harness, cx), Vec::new());
        Ok(())
    });
}

#[gpui::test]
fn the_layer_rests_while_its_window_is_hidden_and_resumes_once_shown(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Hidden);
        frames_over(harness, cx, Duration::from_secs(1));
        assert_eq!(frames_over(harness, cx, Duration::from_secs(1)), 0);

        cx.simulate_window_visibility_change(harness.window(), WindowVisibility::Visible);
        assert!(frames_over(harness, cx, Duration::from_secs(1)) > 0);
        Ok(())
    });
}

#[gpui::test]
fn the_waveform_follows_its_host_when_the_host_moves(cx: &mut TestAppContext) {
    StageBuilder::default().run(cx, |harness, cx| {
        let before = frame(harness, cx)?;
        harness.update(cx, |stage, cx| {
            stage.inset = 120.0;
            cx.notify();
        });
        harness.draw(cx);

        let after = frame(harness, cx)?;
        assert_ne!(after, before);
        assert_eq!(shown(harness, cx), vec![after]);
        Ok(())
    });
}

struct Stage {
    host: Entity<Host>,
    layer: Entity<IndicatorLayer>,
    host_shown: bool,
    host_above_layer: bool,
    inset: f32,
    renders: usize,
}

impl EventEmitter<()> for Stage {}

impl Render for Stage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        let host = self.host_shown.then(|| {
            div()
                .absolute()
                .top(px(self.inset))
                .left(px(self.inset))
                .size(px(200.0))
                .child(self.host.clone().cached(StyleRefinement::default().size_full()))
                .into_any_element()
        });
        let layer: AnyElement = self.layer.clone().into_any_element();
        let layers = if self.host_above_layer { [Some(layer), host] } else { [host, Some(layer)] };
        div().relative().size_full().children(layers.into_iter().flatten())
    }
}

struct Host {
    waveform: bool,
    occluded: bool,
    renders: usize,
}

impl Render for Host {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.renders += 1;
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .debug_selector(|| "waveform-frame".to_owned())
                    .absolute()
                    .top(px(10.0))
                    .left(px(10.0))
                    .when(self.waveform, |frame| frame.child(Waveform::new("waveform", black()))),
            )
            .when(self.occluded, |host| host.child(Backdrop::new("backdrop", UiTheme::new(&ReviewTheme::default()))))
    }
}

#[derive(Default)]
struct StageBuilder {
    host_above_layer: bool,
}

impl StageBuilder {
    const fn host_above_layer(mut self) -> Self {
        self.host_above_layer = true;
        self
    }

    fn run(
        self,
        cx: &mut TestAppContext,
        test: impl FnOnce(&ViewHarness<Stage, ()>, &mut TestAppContext) -> Result<(), Box<dyn Error>>,
    ) {
        self.build(cx).run(cx, test);
    }

    fn build(self, cx: &mut TestAppContext) -> ViewHarness<Stage, ()> {
        let harness = ViewHarness::open(cx, WindowOptions::default(), |window, cx| {
            cx.new(|cx| Stage {
                host: cx.new(|_| Host { waveform: true, occluded: false, renders: 0 }),
                layer: cx.new(|cx| IndicatorLayer::new(window, cx)),
                host_shown: true,
                host_above_layer: self.host_above_layer,
                inset: 20.0,
                renders: 0,
            })
        });
        harness.draw(cx);
        harness
    }
}

fn update_host(harness: &ViewHarness<Stage, ()>, cx: &mut TestAppContext, update: impl FnOnce(&mut Host)) {
    let host = harness.read(cx, |stage, _| stage.host.clone());
    host.update(cx, |host, cx| {
        update(host);
        cx.notify();
    });
    harness.draw(cx);
}

fn host_renders(harness: &ViewHarness<Stage, ()>, cx: &TestAppContext) -> usize {
    harness.read(cx, |stage, cx| stage.host.read(cx).renders)
}

fn shown(harness: &ViewHarness<Stage, ()>, cx: &TestAppContext) -> Vec<Bounds<Pixels>> {
    harness.read(cx, |stage, cx| stage.layer.read(cx).shown())
}

fn frame(harness: &ViewHarness<Stage, ()>, cx: &mut TestAppContext) -> Result<Bounds<Pixels>, Box<dyn Error>> {
    Ok(harness.bounds(cx, "waveform-frame").ok_or("the waveform's frame is painted")?)
}

fn frames_over(harness: &ViewHarness<Stage, ()>, cx: &mut TestAppContext, duration: Duration) -> usize {
    let before = harness.read(cx, |stage, _| stage.renders);
    advance_clock(cx, duration);
    harness.read(cx, |stage, _| stage.renders) - before
}

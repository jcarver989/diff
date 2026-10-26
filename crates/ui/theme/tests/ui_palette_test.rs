use clankerdiff_theme::{
    ButtonVariant, ControlState, DiffPalette, InteractionState, NoticeTone, ReviewTheme, Rgba,
    SelectionState, ThemeError, UiPalette,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
};

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn sage_notices_preserve_semantic_colors() -> TestResult {
    let ui = ReviewTheme::sage()?.ui;
    assert_eq!(
        ui.notice_style(NoticeTone::Info).foreground,
        Rgba::new(130, 177, 204, 255)
    );
    assert_eq!(
        ui.notice_style(NoticeTone::Warning).foreground,
        Rgba::new(216, 181, 106, 255)
    );
    assert_eq!(ui.text_muted, Rgba::new(92, 112, 104, 255));
    assert_ne!(ui.text_secondary, ui.text_muted);
    assert_eq!(
        ui.notice_style(NoticeTone::Neutral).foreground,
        ui.text_secondary
    );
    Ok(())
}

#[test]
fn every_catalog_entry_has_an_explicit_complete_palette() -> TestResult {
    let mut expected: BTreeMap<String, UiPalette> =
        serde_json::from_slice(include_bytes!("../assets/ui.json"))?;
    for (id, bytes) in [
        (
            "sage",
            include_bytes!("../assets/themes/sage.json").as_slice(),
        ),
        (
            "ayu-dark",
            include_bytes!("../assets/themes/ayu-dark.json").as_slice(),
        ),
    ] {
        let document: Value = serde_json::from_slice(bytes)?;
        assert_eq!(document["version"], 3);
        expected.insert(
            id.to_owned(),
            serde_json::from_value(document["ui"].clone())?,
        );
    }
    let catalog = ReviewTheme::catalog();
    assert_eq!(catalog.len(), 34);
    assert_eq!(catalog.iter().filter(|theme| theme.is_dark).count(), 23);
    assert_eq!(
        catalog
            .iter()
            .map(|theme| theme.id.clone())
            .collect::<BTreeSet<_>>(),
        expected.keys().cloned().collect()
    );
    for descriptor in catalog {
        let theme = ReviewTheme::builtin(&descriptor.id)?;
        assert_eq!(theme.ui, expected[&descriptor.id], "{}", descriptor.id);
        assert_ne!(
            theme.ui.text_secondary, theme.ui.text_muted,
            "{}",
            descriptor.id
        );
        assert_ne!(theme.ui.info, theme.ui.warning, "{}", descriptor.id);
        assert_ne!(theme.ui.info, theme.ui.text_muted, "{}", descriptor.id);
        assert_ne!(
            theme.ui.surface, theme.ui.surface_selected,
            "{}",
            descriptor.id
        );
        let restored = ReviewTheme::from_bytes(theme.id().clone(), &theme.to_bytes()?)?;
        assert_eq!(theme.ui, restored.ui);
        assert_eq!(theme.revision(), restored.revision());
        assert_readable(&descriptor.id, theme.ui);
    }
    Ok(())
}

#[test]
fn ui_is_independent_and_every_channel_affects_revision() -> TestResult {
    let theme = ReviewTheme::builder("custom")
        .ui(ReviewTheme::default().ui)
        .diff(DiffPalette {
            foreground: Rgba::new(30, 45, 60, 255),
            background: Rgba::new(240, 245, 250, 255),
            ..DiffPalette::default()
        })
        .build();
    let document: Value = serde_json::from_slice(&theme.to_bytes()?)?;
    let colors = document["ui"].as_object().ok_or("missing UI object")?;
    assert_eq!(colors.len(), theme.ui.colors().len());
    for role in colors.keys() {
        for channel in ["r", "g", "b", "a"] {
            let mut changed = document.clone();
            let value = changed["ui"][role][channel]
                .as_u64()
                .ok_or("missing color channel")?;
            changed["ui"][role][channel] = json!((value + 1) % 256);
            let changed =
                ReviewTheme::from_bytes(theme.id().clone(), &serde_json::to_vec(&changed)?)?;
            assert_ne!(theme.revision(), changed.revision(), "{role}.{channel}");
            assert_eq!(theme.diff, changed.diff);
            assert_eq!(theme.syntax, changed.syntax);
            assert_eq!(theme.markdown, changed.markdown);
        }
    }
    Ok(())
}

#[test]
fn builder_keeps_default_and_explicit_ui_independent_of_diff() {
    let ui = UiPalette {
        info: Rgba::new(1, 2, 3, 255),
        ..UiPalette::default()
    };
    let diff = DiffPalette {
        foreground: Rgba::new(1, 2, 3, 255),
        ..DiffPalette::default()
    };
    let first = ReviewTheme::builder("first")
        .ui(ui)
        .diff(diff.clone())
        .build();
    let second = ReviewTheme::builder("second")
        .diff(diff.clone())
        .ui(ui)
        .build();
    let default_ui = ReviewTheme::builder("default").diff(diff).build().ui;
    assert_eq!(default_ui, UiPalette::default());
    assert_eq!(first.ui, ui);
    assert_eq!(second.ui, ui);
    assert_eq!(first.revision(), second.revision());
}

#[test]
fn theme_documents_require_current_schema_and_complete_ui() -> TestResult {
    let theme = ReviewTheme::default();
    let mut document: Value = serde_json::from_slice(&theme.to_bytes()?)?;
    for version in [2, 99] {
        document["version"] = json!(version);
        assert!(matches!(
            ReviewTheme::from_bytes(theme.id().clone(), &serde_json::to_vec(&document)?),
            Err(ThemeError::UnsupportedVersion { version: actual }) if actual == version
        ));
    }
    document["version"] = json!(3);
    let mut incomplete_ui = document
        .as_object_mut()
        .ok_or("missing document")?
        .remove("ui")
        .ok_or("missing ui")?;
    assert!(matches!(
        ReviewTheme::from_bytes(theme.id().clone(), &serde_json::to_vec(&document)?),
        Err(ThemeError::Parse { .. })
    ));
    incomplete_ui
        .as_object_mut()
        .ok_or("missing ui object")?
        .remove("info");
    for ui in [Value::Null, incomplete_ui] {
        document["ui"] = ui;
        assert!(matches!(
            ReviewTheme::from_bytes(theme.id().clone(), &serde_json::to_vec(&document)?),
            Err(ThemeError::Parse { .. })
        ));
    }
    Ok(())
}

fn assert_readable(id: &str, ui: UiPalette) {
    for (role, foreground) in [
        ("text", ui.text),
        ("secondary", ui.text_secondary),
        ("info", ui.info),
        ("warning", ui.warning),
        ("positive", ui.positive),
        ("destructive", ui.destructive),
        ("accent", ui.accent),
    ] {
        for (surface, background) in [
            ("canvas", ui.canvas),
            ("surface", ui.surface),
            ("hover", ui.surface_hover),
            ("selected", ui.surface_selected),
        ] {
            let ratio = contrast(foreground.over(background), background);
            assert!(ratio >= 4.5, "{id}: {role} on {surface} = {ratio}");
        }
    }
    assert!(
        contrast(ui.text, ui.canvas) > contrast(ui.text_secondary, ui.canvas),
        "{id}"
    );
    assert!(
        contrast(ui.text_secondary, ui.canvas) > contrast(ui.text_muted, ui.canvas),
        "{id}"
    );
    for variant in [
        ButtonVariant::Primary,
        ButtonVariant::Secondary,
        ButtonVariant::Destructive,
    ] {
        let style = ui.control_style(variant, ControlState::new(InteractionState::Rest));
        let background = style.background.unwrap_or(ui.canvas).over(ui.canvas);
        assert!(
            contrast(style.foreground.over(background), background) >= 4.5,
            "{id}: {variant:?}"
        );
    }
    for state in [SelectionState::Selected, SelectionState::Focused] {
        let style = ui.selection_style(state);
        let background = style.background.unwrap_or(ui.canvas).over(ui.canvas);
        assert!(
            contrast(style.foreground.over(background), background) >= 4.5,
            "{id}: {state:?}"
        );
    }
}

fn contrast(left: Rgba, right: Rgba) -> f64 {
    let luminance = |color: Rgba| {
        let channel = |value| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    };
    let left = luminance(left);
    let right = luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

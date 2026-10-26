use clankerdiff_theme::{
    ButtonVariant, ControlState, InteractionState, ReviewTheme, Rgba, SelectionState, ThemeError,
};

fn main() -> Result<(), ThemeError> {
    println!(
        "<!doctype html><meta charset=\"utf-8\"><title>Clankerdiff UI palette gallery</title><style>body{{margin:0;background:#888;font:14px monospace;display:grid;grid-template-columns:repeat(auto-fit,minmax(390px,1fr))}}section{{padding:24px}}p{{padding:8px}}button{{border:0;padding:8px;margin:4px;font:inherit}}h2{{font-size:18px}}</style>"
    );
    for descriptor in ReviewTheme::catalog() {
        let ui = ReviewTheme::builtin(&descriptor.id)?.ui;
        println!(
            "<section style=\"background:{};color:{}\"><h2>{}</h2>",
            css(ui.canvas),
            css(ui.text),
            descriptor.name
        );
        for (label, foreground) in [
            ("Primary content", ui.text),
            ("Secondary metadata", ui.text_secondary),
            ("Muted hints / disabled", ui.text_muted),
            ("Info: running tool", ui.info),
            ("Positive: completed", ui.positive),
            ("Warning: context pressure", ui.warning),
            ("Destructive: failed", ui.destructive),
        ] {
            println!("<p style=\"color:{}\">{label}</p>", css(foreground));
        }
        println!(
            "<p style=\"background:{};border:1px solid {}\">Ordinary input / message surface</p>",
            css(ui.surface),
            css(ui.border)
        );
        println!(
            "<p style=\"background:{}\">Hovered surface</p>",
            css(ui.surface_hover)
        );
        for state in [SelectionState::Selected, SelectionState::Focused] {
            let style = ui.selection_style(state);
            println!(
                "<p style=\"background:{};color:{}\">{state:?} row</p>",
                css(style.background.unwrap_or(ui.canvas)),
                css(style.foreground)
            );
        }
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Destructive,
        ] {
            let style = ui.control_style(variant, ControlState::new(InteractionState::Rest));
            println!(
                "<button style=\"background:{};color:{}\">{variant:?}</button>",
                css(style.background.unwrap_or(ui.canvas)),
                css(style.foreground)
            );
        }
        println!("</section>");
    }
    Ok(())
}

fn css(color: Rgba) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.r, color.g, color.b, color.a
    )
}

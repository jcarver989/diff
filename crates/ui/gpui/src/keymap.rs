use gpui::{Action, App, KeyBinding, KeyBindingContextPredicate};
use std::rc::Rc;

#[must_use]
pub fn key_binding<T: Action>(keystrokes: &str, action: T, context: Option<&str>, cx: &App) -> KeyBinding {
    let predicate = context.map(|context| {
        let predicate = KeyBindingContextPredicate::parse(context)
            .unwrap_or_else(|error| panic!("invalid key context `{context}`: {error}"));
        Rc::new(predicate)
    });
    KeyBinding::load(keystrokes, Box::new(action), predicate, false, None, cx)
        .unwrap_or_else(|error| panic!("invalid keystrokes `{keystrokes}`: {error}"))
}

#[must_use]
pub fn binding_label(binding: &KeyBinding, cx: &App) -> String {
    let os = cx.platform_info().os;
    binding.keystrokes().iter().map(|keystroke| keystroke.display(os).to_string()).collect::<Vec<_>>().join(" ")
}

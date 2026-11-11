use rouge::{
    core::{Chord, ImeEvent, Key, KeyEvent, MenuItem, Point, PointerButton, window_handle},
    reactive::{on_cleanup, sleep, spawn},
};
use std::{path::PathBuf, time::Duration};

pub fn run(script: String, actions: Vec<(String, MenuItem)>) {
    let window = window_handle();
    let task = spawn(async move {
        for line in script.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
            let (command, argument) = line.split_once(' ').unwrap_or((line, ""));
            match command {
                "wait" => sleep(Duration::from_millis(argument.parse().unwrap_or(500))).await,
                "type" => {
                    let text = argument.replace("\\n", "\n");
                    window.run(move |ui| {
                        for (index, line) in text.split('\n').enumerate() {
                            if index > 0 {
                                ui.key_down(KeyEvent::new(Key::Enter));
                            }
                            if !line.is_empty() {
                                ui.ime(ImeEvent::Commit(line.to_owned()));
                            }
                        }
                    });
                }
                "keys" => {
                    for chord in argument.split_whitespace() {
                        match chord.parse::<Chord>() {
                            Ok(chord) => {
                                window.run(move |ui| {
                                    let mut down = KeyEvent::new(chord.key.clone()).with_modifiers(chord.modifiers);
                                    if let Key::Character(text) = &chord.key
                                        && !chord.modifiers.command()
                                    {
                                        down.text = Some(text.clone());
                                    }
                                    ui.key_down(down);
                                    ui.key_up(KeyEvent::new(chord.key).with_modifiers(chord.modifiers));
                                });
                                sleep(Duration::from_millis(50)).await;
                            }
                            Err(error) => eprintln!("script: {error}"),
                        }
                    }
                }
                "action" => {
                    if let Some((_, item)) = actions.iter().find(|(name, _)| name == argument) {
                        window.dispatch_menu_item(item.clone());
                    } else {
                        eprintln!("script: unknown action `{argument}`");
                    }
                }
                "click" => {
                    let mut args = argument.split_whitespace();
                    if let (Some(x), Some(y)) = (coordinate(args.next()), coordinate(args.next())) {
                        let count = args.next().and_then(|count| count.parse::<u8>().ok()).unwrap_or(1);
                        window.run(move |ui| {
                            ui.pointer_moved(Point::new(x, y));
                            for _ in 0..count {
                                ui.pointer_down(PointerButton::Primary);
                                ui.pointer_up(PointerButton::Primary);
                            }
                        });
                    }
                }
                "drag" => {
                    let mut args = argument.split_whitespace();
                    if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                        coordinate(args.next()),
                        coordinate(args.next()),
                        coordinate(args.next()),
                        coordinate(args.next()),
                    ) {
                        window.run(move |ui| {
                            let (from, to) = (Point::new(x1, y1), Point::new(x2, y2));
                            ui.pointer_moved(from);
                            ui.pointer_down(PointerButton::Primary);
                            for step in 1..=8_u8 {
                                ui.pointer_moved(Point::new(
                                    from.x + (to.x - from.x) * (f32::from(step) / 8.0),
                                    from.y + (to.y - from.y) * (f32::from(step) / 8.0),
                                ));
                            }
                            ui.pointer_up(PointerButton::Primary);
                        });
                    }
                }
                "shot" => window.screenshot(PathBuf::from(argument)),
                "quit" => {
                    window.quit();
                    return;
                }
                _ => eprintln!("script: unknown step `{line}`"),
            }
            sleep(Duration::from_millis(16)).await;
        }
    });
    on_cleanup(move || task.cancel());
}

fn coordinate(value: Option<&str>) -> Option<f32> {
    value?.parse::<f32>().ok().filter(|value| value.is_finite())
}

use rouge_core::{Chord, ChordError};

pub fn key_binding(keystroke: &str) -> Result<Chord, ChordError> {
    Chord::parse(keystroke)
}

#[must_use]
pub fn binding_label(binding: &Chord) -> String {
    binding.display()
}

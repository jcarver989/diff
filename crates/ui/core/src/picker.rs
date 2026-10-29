use crate::fuzzy::fuzzy_filter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerOption {
    pub text: String,
    pub group: Option<String>,
    pub disabled: bool,
}

impl PickerOption {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), group: None, disabled: false }
    }

    #[must_use]
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerRow<'a> {
    Group(&'a str),
    Option { position: usize, index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerState {
    options: Vec<PickerOption>,
    filtered: bool,
    query: String,
    matches: Vec<usize>,
    highlighted: usize,
}

impl PickerState {
    #[must_use]
    pub fn new(options: Vec<PickerOption>) -> Self {
        let mut state = Self { options, filtered: true, query: String::new(), matches: Vec::new(), highlighted: 0 };
        state.refilter();
        state
    }

    #[must_use]
    pub fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self.refilter();
        self
    }

    #[must_use]
    pub const fn is_filtered(&self) -> bool {
        self.filtered
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    pub fn set_options(&mut self, options: Vec<PickerOption>) {
        self.options = options;
        self.refilter();
    }

    #[must_use]
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    #[must_use]
    pub const fn highlighted_position(&self) -> usize {
        self.highlighted
    }

    #[must_use]
    pub fn highlighted(&self) -> Option<usize> {
        self.matches.get(self.highlighted).copied().filter(|&index| !self.options[index].disabled)
    }

    pub fn highlight(&mut self, index: usize) -> bool {
        let position = self.matches.iter().position(|&matched| matched == index);
        position.is_some_and(|position| self.highlight_position(position))
    }

    pub fn highlight_position(&mut self, position: usize) -> bool {
        let enabled = self.matches.get(position).is_some_and(|&index| !self.options[index].disabled);
        if enabled {
            self.highlighted = position;
        }
        enabled
    }

    pub fn move_by(&mut self, delta: isize) -> bool {
        let Some(last) = self.matches.len().checked_sub(1) else {
            return false;
        };
        let target = self.highlighted.saturating_add_signed(delta).min(last);
        let step = if delta < 0 { -1 } else { 1 };
        let Some(next) = self.enabled_from(target, step).or_else(|| self.enabled_from(target, -step)) else {
            return false;
        };
        let moved = next != self.highlighted;
        self.highlighted = next;
        moved
    }

    #[must_use]
    pub fn rows(&self) -> Vec<PickerRow<'_>> {
        let mut rows = Vec::with_capacity(self.matches.len());
        let mut group = None;
        for (position, &index) in self.matches.iter().enumerate() {
            let option_group = self.options[index].group.as_deref();
            if option_group != group {
                rows.extend(option_group.map(PickerRow::Group));
                group = option_group;
            }
            rows.push(PickerRow::Option { position, index });
        }
        rows
    }

    fn refilter(&mut self) {
        self.matches = if self.filtered {
            fuzzy_filter(&self.query, self.options.iter().map(|option| &option.text))
        } else {
            (0..self.options.len()).collect()
        };
        let options = &self.options;
        self.matches.sort_by_key(|&index| {
            options[index]
                .group
                .as_ref()
                .map(|group| options.iter().position(|option| option.group.as_ref() == Some(group)))
        });
        self.highlighted = self.enabled_from(0, 1).unwrap_or(0);
    }

    fn enabled_from(&self, start: usize, step: isize) -> Option<usize> {
        let mut position = start;
        loop {
            if !self.options[*self.matches.get(position)?].disabled {
                return Some(position);
            }
            position = position.checked_add_signed(step)?;
        }
    }
}

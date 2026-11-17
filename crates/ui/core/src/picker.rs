use crate::fuzzy::fuzzy_filter;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerOption {
    pub text: String,
    pub group: Option<String>,
    pub disabled: bool,
}

impl PickerOption {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickerRow {
    Group(usize),
    Option(usize),
}

impl PickerRow {
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Group(index) | Self::Option(index) => index,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerState {
    options: Vec<PickerOption>,
    groups: Vec<Option<usize>>,
    filtered: bool,
    limit: Option<usize>,
    query: String,
    matches: Vec<usize>,
    positions: Vec<Option<usize>>,
    highlighted: usize,
}

impl PickerState {
    #[must_use]
    pub fn new(options: Vec<PickerOption>) -> Self {
        let mut state = Self {
            options: Vec::new(),
            groups: Vec::new(),
            filtered: true,
            limit: None,
            query: String::new(),
            matches: Vec::new(),
            positions: Vec::new(),
            highlighted: 0,
        };
        state.set_options(options);
        state
    }

    #[must_use]
    pub fn unfiltered(mut self) -> Self {
        self.filtered = false;
        self.refilter();
        self
    }

    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self.refilter();
        self
    }

    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
        self.refilter();
    }

    #[must_use]
    pub fn options(&self) -> &[PickerOption] {
        &self.options
    }

    pub fn set_options(&mut self, options: Vec<PickerOption>) {
        self.options = options;
        let mut order = HashMap::new();
        self.groups = self
            .options
            .iter()
            .map(|option| {
                option.group.as_deref().map(|group| {
                    let next = order.len();
                    *order.entry(group).or_insert(next)
                })
            })
            .collect();
        self.refilter();
    }

    #[must_use]
    pub fn matches(&self) -> &[usize] {
        &self.matches
    }

    #[must_use]
    pub fn highlighted(&self) -> Option<usize> {
        self.matches.get(self.highlighted).copied().filter(|&index| !self.options[index].disabled)
    }

    pub fn highlight(&mut self, index: usize) -> bool {
        let Some(position) = self.positions.get(index).copied().flatten() else {
            return false;
        };
        let enabled = !self.options[index].disabled;
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
    pub fn rows(&self) -> Vec<PickerRow> {
        let mut rows = Vec::with_capacity(self.matches.len());
        let mut group = None;
        for &index in &self.matches {
            let option_group = self.options[index].group.as_deref();
            if option_group != group {
                if option_group.is_some() {
                    rows.push(PickerRow::Group(index));
                }
                group = option_group;
            }
            rows.push(PickerRow::Option(index));
        }
        rows
    }

    fn refilter(&mut self) {
        if self.filtered {
            let keys = self.options.iter().map(|option| option.text.as_str());
            fuzzy_filter(&self.query, keys, &mut self.matches);
        } else {
            self.matches.clear();
            self.matches.extend(0..self.options.len());
        }
        let groups = &self.groups;
        self.matches.sort_by_key(|&index| groups[index]);
        if let Some(limit) = self.limit {
            self.matches.truncate(limit);
        }
        self.positions.clear();
        self.positions.resize(self.options.len(), None);
        for (position, &index) in self.matches.iter().enumerate() {
            self.positions[index] = Some(position);
        }
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

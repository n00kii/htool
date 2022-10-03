use std::{cell::RefCell, rc::Rc};
use anyhow::Result;

pub struct PollBuffer<T> {
    pub entries: Vec<Rc<RefCell<T>>>,
    pub size_limit: Option<usize>,
    pub count_limit: Option<usize>,
    pub on_add: fn(&Rc<RefCell<T>>),
    pub on_poll: fn(&Rc<RefCell<T>>) -> bool,
    pub get_entry_size: fn(&Rc<RefCell<T>>) -> usize,
}

impl<T: PartialEq> PollBuffer<T> {
    fn contains_entry(&self, entry: &Rc<RefCell<T>>) -> bool {
        self.entries.contains(entry)
    }
    fn current_size(&self) -> usize {
        self.entries.iter().map(|entry| (self.get_entry_size)(entry)).sum()
    }
    pub fn is_full(&self) -> bool {
        let is_full_by_size = if let Some(size_limit) = self.size_limit.as_ref() {
            self.current_size() > *size_limit
        } else {
            false
        };

        let is_full_by_count = if let Some(count_limit) = self.count_limit.as_ref() {
            self.entries.len() == *count_limit
        } else {
            false
        };

        is_full_by_count || is_full_by_size
    }
    pub fn try_add_entry(&mut self, entry: Rc<RefCell<T>>) -> Result<()> {
        if self.contains_entry(&entry) {
            return Err(anyhow::Error::msg("already added"));
        }

        if let Some(size_limit) = self.size_limit.as_ref() {
            if (self.current_size() + (self.get_entry_size)(&entry) > *size_limit) && !self.entries.is_empty() {
                return Err(anyhow::Error::msg("buffer full (size)"));
            }
        }

        if let Some(count_limit) = self.count_limit.as_ref() {
            if self.entries.len() == *count_limit {
                return Err(anyhow::Error::msg("buffer full (count)"));
            }
        }

        (self.on_add)(&entry);
        self.entries.push(entry);
        Ok(())
    }
    pub fn poll(&mut self) {
        self.entries.retain_mut(|entry| (self.on_poll)(entry))
    }
    pub fn new(
        size_limit: Option<usize>,
        count_limit: Option<usize>,
        on_add: Option<fn(&Rc<RefCell<T>>)>,
        on_poll: Option<fn(&Rc<RefCell<T>>) -> bool>,
        get_entry_size: Option<fn(&Rc<RefCell<T>>) -> usize>,
    ) -> Self {
        Self {
            size_limit,
            count_limit,
            on_add: on_add.unwrap_or(PollBuffer::default_on_add),
            on_poll: on_poll.unwrap_or(PollBuffer::default_on_poll),
            get_entry_size: get_entry_size.unwrap_or(PollBuffer::default_get_entry_size),
            entries: vec![],
        }
    }

    fn default_on_add(t: &Rc<RefCell<T>>) {}
    fn default_on_poll(t: &Rc<RefCell<T>>) -> bool {
        true
    }
    fn default_get_entry_size(t: &Rc<RefCell<T>>) -> usize {
        0
    }
}

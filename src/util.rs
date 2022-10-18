use anyhow::Result;
use parking_lot::Mutex;
use poll_promise::Promise;
use std::{cell::RefCell, rc::Rc, sync::Arc};

pub struct PollBuffer<T> {
    pub entries: Vec<Rc<RefCell<T>>>,
    pub size_limit: Option<usize>,
    pub count_limit: Option<usize>,
    pub on_add: fn(&Rc<RefCell<T>>),
    pub on_poll: fn(&Rc<RefCell<T>>) -> bool,
    pub get_entry_size: fn(&Rc<RefCell<T>>) -> usize,
}

// #[derive(PartialEq)]
pub enum BufferEntry<T>
// where
//     parking_lot::lock_api::Mutex<parking_lot::RawMutex, T>: PartialEq,
{
    NotSync(Rc<RefCell<T>>),
    Sync(Arc<Mutex<T>>),
}

pub struct PollBuffer2<T> {
    pub entries: Vec<BufferEntry<T>>,
    pub size_limit: Option<usize>,
    pub count_limit: Option<usize>,
    pub on_add: fn(&BufferEntry<T>),
    pub on_poll: fn(&BufferEntry<T>) -> bool,
    pub get_entry_size: fn(&BufferEntry<T>) -> usize,
}

impl<T: PartialEq> PollBuffer2<T> {
    fn contains_entry(&self, entry: &BufferEntry<T>) -> bool {
        self.entries.iter().any(|other_entry| {
            match entry {
                BufferEntry::NotSync(entry) => {
                    if let BufferEntry::NotSync(other_entry) = other_entry {
                        entry == other_entry
                    } else {
                        false
                    }
                }
                BufferEntry::Sync(entry) => {
                    if let BufferEntry::Sync(other_entry) = other_entry {
                        entry.data_ptr() == other_entry.data_ptr()
                    } else {
                        false
                    }
                }
            }
        })
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
    pub fn try_add_entry(&mut self, entry: BufferEntry<T>) -> Result<()> {
        if let Some(count_limit) = self.count_limit.as_ref() {
            if self.entries.len() == *count_limit {
                return Err(anyhow::Error::msg("buffer full (count)"));
            }
        }

        if let Some(size_limit) = self.size_limit.as_ref() {
            if (self.current_size() + (self.get_entry_size)(&entry) > *size_limit) && !self.entries.is_empty() {
                return Err(anyhow::Error::msg("buffer full (size)"));
            }
        }

        if self.contains_entry(&entry) {
            return Err(anyhow::Error::msg("already added"));
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
        on_add: Option<fn(&BufferEntry<T>)>,
        on_poll: Option<fn(&BufferEntry<T>) -> bool>,
        get_entry_size: Option<fn(&BufferEntry<T>) -> usize>,
    ) -> Self {
        Self {
            size_limit,
            count_limit,
            on_add: on_add.unwrap_or(Self::default_on_add),
            on_poll: on_poll.unwrap_or(Self::default_on_poll),
            get_entry_size: get_entry_size.unwrap_or(Self::default_get_entry_size),
            entries: vec![],
        }
    }

    fn default_on_add(_t: &BufferEntry<T>) {}
    fn default_on_poll(_t: &BufferEntry<T>) -> bool {
        true
    }
    fn default_get_entry_size(_t: &BufferEntry<T>) -> usize {
        0
    }
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
        if let Some(count_limit) = self.count_limit.as_ref() {
            if self.entries.len() == *count_limit {
                return Err(anyhow::Error::msg("buffer full (count)"));
            }
        }

        if let Some(size_limit) = self.size_limit.as_ref() {
            if (self.current_size() + (self.get_entry_size)(&entry) > *size_limit) && !self.entries.is_empty() {
                return Err(anyhow::Error::msg("buffer full (size)"));
            }
        }

        if self.contains_entry(&entry) {
            return Err(anyhow::Error::msg("already added"));
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

    fn default_on_add(_t: &Rc<RefCell<T>>) {}
    fn default_on_poll(_t: &Rc<RefCell<T>>) -> bool {
        true
    }
    fn default_get_entry_size(_t: &Rc<RefCell<T>>) -> usize {
        0
    }
}

pub fn is_opt_promise_ready<T: Send>(opt_promise: &Option<Promise<T>>) -> bool {
    if let Some(promise) = opt_promise.as_ref() {
        promise.ready().is_some()
    } else {
        false
    }
}

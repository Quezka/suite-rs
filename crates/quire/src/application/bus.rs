//! Framework-free change notifications, so outer layers can react to writes (port of `bus.py`).

use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Topic {
    Courses,
    Events,
    Ideas,
    Tasks,
    Notes,
    Journal,
    School,
    Work,
    Focus,
    Sleep,
    Water,
    Medicine,
}

type Listener = Box<dyn Fn(Topic)>;

/// Single-threaded on purpose: use cases run on the UI thread (sqlite connections are thread-bound).
#[derive(Default)]
pub struct ChangeBus {
    listeners: RefCell<Vec<Listener>>,
}

impl ChangeBus {
    pub fn subscribe(&self, listener: impl Fn(Topic) + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn publish(&self, topic: Topic) {
        for l in self.listeners.borrow().iter() {
            l(topic);
        }
    }
}

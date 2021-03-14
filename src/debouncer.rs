//! Debouncer & access code
use std::{collections::HashMap, path::PathBuf, sync::{Arc, Mutex, mpsc::{self, Receiver}}, time::{Duration, Instant}};

use crate::{Error, Event, EventKind, RecommendedWatcher, Watcher};

struct EventData {
    event: EventKind,
    time: Instant,
}

impl From<EventKind> for EventData {
    fn from(e: EventKind) -> Self {
        EventData {
            event: e,
            time: Instant::now(),
        }
    }
}

/// Creates a new debounced watcher
pub fn new_debouncer(timeout: Duration) -> Result<(Receiver<HashMap<PathBuf,EventKind>>,RecommendedWatcher), Error> {
    let events: Arc<Mutex<HashMap<PathBuf,EventData>>> = Arc::new(Mutex::new(HashMap::new()));
    
    let (tx,rx) = mpsc::channel();

    let events_c = events.clone();
    std::thread::spawn(move ||{
        loop {
            std::thread::sleep(timeout);
            let mut data = HashMap::new();
            {
                let mut lck = events_c.lock().expect("Can't lock events map");
                
                let mut data_back = HashMap::new();
                // TODO: use drain_filter if stabilized
                for (k,v) in lck.drain() {
                    if v.time.elapsed() >= timeout {
                        data.insert(k,v.event);
                    } else {
                        data_back.insert(k,v);
                    }
                }
                *lck = data_back;
            }

            if tx.send(data).is_err() {
                break;
            }
        }
    });

    let watcher = RecommendedWatcher::new_immediate(move |e: Result<Event, Error>| {
        let mut lock = events.lock().expect("Can't lock mutex!");
        if let Ok(v) = e {
            match &v.kind {
                EventKind::Any | EventKind::Other => {
                    for p in v.paths.into_iter() {
                        if let Some(existing) = lock.get(&p) {
                            // TODO: consider EventKind::Any
                            match existing.event {
                                EventKind::Any | EventKind::Other => (),
                                _ => continue,
                            }
                        }
                        lock.insert(p, v.kind.clone().into());
                    }
                },
                EventKind::Access(_t) => {
                    for p in v.paths.into_iter() {
                        if let Some(existing) = lock.get(&p) {
                            // TODO: consider EventKind::Any
                            match existing.event {
                                EventKind::Access(_) | EventKind::Any | EventKind::Other => (),
                                _ => continue,
                            }
                        }
                        lock.insert(p, v.kind.clone().into());
                    }
                },
                EventKind::Modify(_t) => {
                    for p in v.paths.into_iter() {
                        if let Some(existing) = lock.get(&p) {
                            // TODO: consider EventKind::Any on invalid configurations
                            match existing.event {
                                EventKind::Access(_) | EventKind::Any | EventKind::Other => (),
                                _ => continue,
                            }
                        }
                        lock.insert(p, v.kind.clone().into());
                    }
                },
                // ignore previous events, override
                EventKind::Create(_) | EventKind::Remove(_) => {
                    for p in v.paths.into_iter() {
                        lock.insert(p, v.kind.clone().into());
                    }
                },
            }
        }
    })?;


    Ok((rx,watcher))
}
mod config;
mod service;

pub use service::run_app;

pub trait Store {
    fn save(&self, value: Config);
}

pub struct MemoryStore;

impl Store for MemoryStore {
    fn save(&self, value: Config) {
        persist(value);
    }
}

pub use config::Config;

fn persist(value: Config) {
    let _ = value.name;
}

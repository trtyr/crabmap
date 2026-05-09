use crate::config::{Config, load_config};
use crate::{MemoryStore, Store};

pub fn run_app() -> Config {
    let config = load_config();
    MemoryStore.save(load_config());
    config
}

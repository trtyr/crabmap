pub struct Config {
    pub name: String,
    pub mode: ConfigMode,
}

pub enum ConfigMode {
    Fast,
    Custom(String),
}

pub fn load_config() -> Config {
    Config {
        name: "demo".to_string(),
        mode: ConfigMode::Fast,
    }
}

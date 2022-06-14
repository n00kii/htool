use custom_error::custom_error;
use thiserror::Error;
// use figment::Error;

custom_error! { pub MediaReadError
    UndefinedPath = "no path was set before trying to read media",
    NoMoreDirectoryEntries = "directory entry iterator exhausted",
}

// custom_error! { pub ConfigError
//     ReadError = "couldn't read config",
//     SaveError = "couldn't write config",
//     SerializationError = "couldn't write config",
// }


// #[derive(Error, Debug)]
// pub enum ConfigError {
//     #[error("couldn't read config")]
//     ReadError(#[from] figment::Error),

//     #[error("couldn't write config")]
//     SaveError,
    
//     #[error("couldn't serialize config")]
//     SerializationError,

//     #[error("couldn't deserialize config")]
//     DeserializationError,
// }
pub mod errors;
extern crate sled;
use anyhow::{bail, Result};
use lazy_static::lazy_static;
use sled::Db;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::errors::{KeyValueStoreError, KeyValueStoreErrorCodes};
static DATABASE_STORE_FIILE_PATH: &str = "~/.mecha/agent/storage/key_value_store";
// Singleton database connection
lazy_static! {

    static ref DATABASE: Arc<Mutex<Db>> = Arc::new(Mutex::new(
        //TODO: change this path to be dynamic
        sled::open(DATABASE_STORE_FIILE_PATH).unwrap()
    ));
}

pub struct KeyValueStoreClient;

impl KeyValueStoreClient {
    pub fn new() -> Self {
        let _r = check_dir_if_exist(DATABASE_STORE_FIILE_PATH);
        KeyValueStoreClient
    }
    pub fn set(&mut self, key: &str, value: &str) -> Result<bool> {
        let db = DATABASE.lock().expect("Failed to acquire lock");
        let _last_inserted = db.insert(key, value);

        Ok(true)
    }
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let db = DATABASE.lock().expect("Failed to acquire lock");
        let last_inserted = db.get(key);
        match last_inserted {
            Ok(s) => Ok(s.map(|s| String::from_utf8(s.to_vec()).unwrap())),
            Err(e) => bail!(KeyValueStoreError::new(
                KeyValueStoreErrorCodes::RetrieveValueError,
                format!("Error retrieving value from db - {}", e),
                true
            )),
        }
    }
}

fn check_dir_if_exist(storage_file_path: &str) -> Result<()> {
    println!("check_dir_if_exist: {:?}", storage_file_path);
    let path = PathBuf::from(&storage_file_path);
    if !path.exists() {
        println!("path not exist");
        match mkdirp::mkdirp(&storage_file_path) {
            Ok(p) => {
                println!("path created {:?}", p);
                p
            }
            Err(err) => bail!(err),
        };
    }
    Ok(())
}

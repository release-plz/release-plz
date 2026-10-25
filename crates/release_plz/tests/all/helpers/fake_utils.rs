use std::{collections::hash_map::RandomState, hash::BuildHasher};

pub fn fake_id() -> String {
    format!("test{:016x}", RandomState::new().hash_one(()))
}

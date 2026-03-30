use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct ArchiveData<'a> {
    pub users: HashMap<u64, MinUser<'a>>,
    pub channels: Vec<ChannelMeta<'a>>,
    pub messages: HashMap<u64, Vec<MinMsg<'a>>>,
}

#[derive(Serialize, Deserialize)]
pub struct ChannelMeta<'a> {
    pub id: u64,
    pub n: Cow<'a, str>,
    pub c: Cow<'a, str>,
}

#[derive(Serialize, Deserialize)]
pub struct MinUser<'a> {
    pub n: Cow<'a, str>, // Display name (Nickname or Username)
    pub u: Cow<'a, str>, // Actual Username for searching
    pub c: Option<Cow<'a, str>>,
    pub p: Option<Cow<'a, str>>,
}

#[derive(Serialize, Deserialize)]
pub struct MinMsg<'a> {
    pub i: u64,
    pub a: u64,
    pub c: Cow<'a, str>,
    pub t: i64,
    pub p: bool,
    pub r: Option<u64>,
}

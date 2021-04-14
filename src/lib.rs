use anyhow::{Context, Error as AnyError, Result};
use arrayvec::ArrayVec;
use quick_xml::events::BytesText;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
struct Guid(ArrayVec<u8, 36>); // "8bff0fc0-0ad4-40a4-a4c7-c6a5c1df96b7"

impl PartialEq for Guid {
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}
impl Eq for Guid {}

impl Hash for Guid {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl TryFrom<&BytesText<'_>> for Guid {
    type Error = AnyError;
    fn try_from(value: &BytesText<'_>) -> Result<Self, Self::Error> {
        Ok(Self(ArrayVec::try_from(&**value).with_context(|| {
            format!("GUID didn't fit into array {:?}", value)
        })?))
    }
}

impl<'a> Borrow<[u8]> for Guid {
    fn borrow(&self) -> &[u8] {
        &*(self.0)
    }
}

pub struct GuidMap {
    map: HashMap<Guid, u32>,
    next: u32,
}

impl GuidMap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next: 0,
        }
    }
    pub fn get_or_insert(&mut self, txt: &BytesText) -> Result<u32> {
        Ok(if let Some(id) = self.map.get(&**txt) {
            *id
        } else {
            self.next += 1;
            self.map.insert(txt.try_into()?, self.next);
            self.next
        })
    }
}

impl Default for GuidMap {
    fn default() -> Self {
        Self::new()
    }
}

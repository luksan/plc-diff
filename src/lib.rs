pub mod grafcet;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::Hash;
use std::path::Path;

use anyhow::{Context, Error as AnyError, Result};
use arrayvec::ArrayVec;
use quick_xml::events::{BytesText, Event};
use quick_xml::Reader;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CurrentTag {
    Address,
    Id,
    To,
    From,
    GrafcetNodeStep,
    GrafcetOrFork,
    GrafcetOrJunction,
    GrafcetTransition,
    InstructionLine,
    InstructionLineEntity,
    MainComment,
    Name,
    LadderElements,
    RungEntity,
    Symbol,
    Other,
    None,
}

impl Default for CurrentTag {
    fn default() -> Self {
        Self::None
    }
}

impl From<&[u8]> for CurrentTag {
    fn from(tag: &[u8]) -> Self {
        match tag {
            b"Address" => Self::Address,
            b"Id" => Self::Id,
            b"From" => Self::From,
            b"To" => Self::To,
            b"GrafcetNodeStep" => Self::GrafcetNodeStep,
            b"GrafcetOrFork" => Self::GrafcetOrFork,
            b"GrafcetOrJunction" => Self::GrafcetOrJunction,
            b"GrafcetTransition" => Self::GrafcetTransition,
            b"InstructionLine" => Self::InstructionLine,
            b"InstructionLineEntity" => Self::InstructionLineEntity,
            b"LadderElements" => Self::LadderElements,
            b"MainComment" => Self::MainComment,
            b"Name" => Self::Name,
            b"RungEntity" => Self::RungEntity,
            b"Symbol" => Self::Symbol,
            _ => Self::Other,
        }
    }
}

#[derive(Default, Clone, Hash, PartialEq, Eq)]
pub struct Guid(ArrayVec<u8, 36>); // "8bff0fc0-0ad4-40a4-a4c7-c6a5c1df96b7"

impl AsRef<[u8]> for Guid {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}
impl Debug for Guid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({})", std::str::from_utf8(self.as_ref()).unwrap())
    }
}
impl Display for Guid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", std::str::from_utf8(self.as_ref()).unwrap())
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

pub fn process_file(smbp_file: &Path, visitors: &mut [&mut dyn XmlNodeVisitor]) -> Result<()> {
    let mut reader =
        Reader::from_file(smbp_file).context("Failed to create xml reader from path")?;

    let mut read_buf = Vec::new();
    let mut current_tag = Default::default();
    loop {
        let mut ev = reader.read_event(&mut read_buf)?;
        match &ev {
            Event::Start(start) => current_tag = start.local_name().into(),
            Event::End(end) => current_tag = end.local_name().into(),
            _ => {}
        };
        let orig = ev.clone();
        for visitor in &mut *visitors {
            ev = match visitor.visit(ev, current_tag)? {
                VisitProcessing::Continue(event) => event,
                VisitProcessing::NextNode => break,
            };
        }
        if matches!(orig, Event::End(_)) {
            current_tag = CurrentTag::None;
        }

        if matches!(orig, Event::Eof) {
            break;
        }
        read_buf.clear();
    }
    Ok(())
}

pub trait XmlNodeVisitor {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> VisitResult<'a>;
}

impl<T> XmlNodeVisitor for T
where
    T: for<'b> FnMut(Event<'b>, CurrentTag) -> VisitResult<'b>,
{
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> VisitResult<'a> {
        self(event, current)
    }
}

pub enum VisitProcessing<'a> {
    /// Let the next visitor (if any) process the (possibly modified) node
    Continue(Event<'a>),
    /// Skip all remaining visitors and read in the next node
    NextNode,
}

pub type VisitResult<'a> = Result<VisitProcessing<'a>>;

#[cfg(test)]
mod test {
    use std::path::Path;

    use super::*;

    struct NodeCounter(usize);
    impl XmlNodeVisitor for NodeCounter {
        fn visit<'a>(&mut self, event: Event<'a>, _curr: CurrentTag) -> VisitResult<'a> {
            self.0 += 1;
            Ok(VisitProcessing::Continue(event))
        }
    }

    #[test]
    fn test_xml_visitor() {
        let mut counter = NodeCounter(0);

        process_file(
            &Path::new("tests/orig.smbp"),
            &mut [
                // Node visitors
                &mut counter,
            ],
        )
        .unwrap();

        println!("Total xml nodes processed: {}", counter.0)
    }
}

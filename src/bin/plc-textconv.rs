use std::collections::HashMap;
use std::convert::{TryFrom, TryInto};
use std::env;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::BufWriter;
use std::mem::take;
use std::path::Path;

use anyhow::{Context, Result};
use arrayvec::ArrayVec;
use itertools::Itertools;
use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::Writer;

use plc_diff::grafcet::{GrafcetCounter, GrafcetTracer};
use plc_diff::{
    process_file, CurrentTag, Guid, GuidMap, VisitProcessing, VisitResult, XmlNodeVisitor,
};

#[derive(Debug)]
struct NormalizeInstructionLine<'a> {
    in_entity: bool,
    text: Vec<u8>,
    names: &'a IoNames,
}

impl<'a> NormalizeInstructionLine<'a> {
    fn new(names: &'a IoNames) -> Self {
        Self {
            in_entity: false,
            text: Vec::new(),
            names,
        }
    }

    fn normalize_text(&self, txt: &BytesText) -> Vec<u8> {
        let mut new = Vec::new();
        for word in (*txt).split(|c| c.is_ascii_whitespace()) {
            if word.is_empty() {
                continue;
            }
            new.extend_from_slice(word);
            if let Some(symbol) = self.names.get_symbol(word) {
                new.resize(new.len() + 1 + 13usize.saturating_sub(new.len()), b' ');
                new.push(b'[');
                new.extend_from_slice(symbol);
                new.push(b']');
            }
            new.push(b' ');
        }
        new.pop();
        new
    }
}

impl XmlNodeVisitor for NormalizeInstructionLine<'_> {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> Result<VisitProcessing<'a>> {
        match &event {
            Event::Start(_) if current == CurrentTag::InstructionLineEntity => {
                self.in_entity = true;
            }

            _ if !self.in_entity => return Ok(VisitProcessing::Continue(event)),

            Event::End(_) if current == CurrentTag::InstructionLineEntity => {
                self.in_entity = false;
                let text = std::mem::replace(&mut self.text, Vec::new());
                return Ok(VisitProcessing::Continue(Event::Text(
                    BytesText::from_escaped(text),
                )));
            }
            Event::Text(txt) => {
                let mut new = self.normalize_text(txt);
                if !self.text.is_empty() && !new.is_empty() {
                    self.text.push(b'\t');
                }
                self.text.append(&mut new);
            }
            _ => {}
        }
        Ok(VisitProcessing::NextNode)
    }
}

struct GuidVisitor {
    map: GuidMap,
}

impl GuidVisitor {
    fn new() -> Self {
        Self {
            map: GuidMap::new(),
        }
    }
}

impl XmlNodeVisitor for GuidVisitor {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> Result<VisitProcessing<'a>> {
        let event = match event {
            Event::Text(txt)
                if matches!(current, CurrentTag::From | CurrentTag::To | CurrentTag::Id) =>
            {
                let new = self.map.get_or_insert(&txt)?;
                Event::Text(BytesText::from_escaped_str(format!("=={}==", new)))
            }
            _ => event,
        };
        Ok(VisitProcessing::Continue(event))
    }
}

struct SkipTag {
    skipping: bool,
    tag: CurrentTag,
}

impl SkipTag {
    fn new(tag: CurrentTag) -> Self {
        Self {
            skipping: false,
            tag,
        }
    }
}

impl XmlNodeVisitor for SkipTag {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> Result<VisitProcessing<'a>> {
        if current != self.tag && self.skipping {
            return Ok(VisitProcessing::NextNode);
        }
        if current == self.tag {
            match &event {
                Event::Start(_) => self.skipping = true,
                Event::End(_) => self.skipping = false,
                _ => {}
            };
        }
        Ok(VisitProcessing::Continue(event))
    }
}

struct EventWriter<T: std::io::Write>(Writer<T>);
impl<T: std::io::Write> XmlNodeVisitor for EventWriter<T> {
    fn visit<'a>(&mut self, event: Event<'a>, _: CurrentTag) -> VisitResult<'a> {
        self.0.write_event(&event)?;
        Ok(VisitProcessing::Continue(event))
    }
}

#[derive(Debug, Default)]
struct IoNames {
    names: HashMap<ArrayVec<u8, 30>, ArrayVec<u8, 30>>,
    new_address: (usize, ArrayVec<u8, 30>),
    depth: usize,
}
impl IoNames {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn get_symbol(&self, address: &[u8]) -> Option<&[u8]> {
        self.names.get(address).map(|v| v.as_ref())
    }
}
impl XmlNodeVisitor for IoNames {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> VisitResult<'a> {
        match &event {
            Event::Start(_) => self.depth += 1,
            Event::End(_) => {
                self.depth -= 1;
                if self.depth + 2 < self.new_address.0 {
                    take(&mut self.new_address);
                }
            }
            Event::Text(txt) if current == CurrentTag::Address => {
                self.new_address = (
                    self.depth,
                    ArrayVec::try_from(&**txt).with_context(|| format!("{:?}", event))?,
                );
            }
            Event::Text(txt) if current == CurrentTag::Symbol => {
                let (_, address) = take(&mut self.new_address);
                self.names.insert(
                    address,
                    ArrayVec::try_from(&**txt).with_context(|| format!("{:?}", event))?,
                );
            }
            _ => {}
        }
        Ok(VisitProcessing::Continue(event))
    }
}

#[derive(Debug, Default)]
struct Rung {
    name: Vec<u8>,
    main_comment: Vec<u8>,
}
#[derive(Debug, Default)]
struct NameTracker {
    rungs: Vec<Rung>,
    ids: HashMap<Guid, String>,
    names: Vec<(usize, String)>,
    new_comment: Vec<u8>,
    new_id: Guid,
    depth: usize,
}
impl NameTracker {
    fn mk_rung_name(&self) -> Vec<u8> {
        self.names
            .iter()
            .skip(1) // Skip the project name
            .take_while(|(depth, _)| depth <= &(self.depth + 2))
            .map(|(_, name)| name.as_str())
            .join(" > ")
            .into()
    }
    fn latest_name(&self) -> String {
        self.names
            .last()
            .map_or_else(String::new, |(_, name)| name.clone())
    }
    fn remove_old_names(&mut self) {
        while self
            .names
            .last()
            .map_or(false, |(depth, _)| depth >= &self.depth)
        {
            self.names.pop();
        }
    }
}
impl XmlNodeVisitor for NameTracker {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> VisitResult<'a> {
        match &event {
            Event::Text(txt) => match current {
                CurrentTag::Id => self.new_id = txt.try_into()?,
                CurrentTag::MainComment => self.new_comment = txt.to_vec(),
                CurrentTag::Name => {
                    self.remove_old_names();
                    self.names
                        .push((self.depth, std::str::from_utf8(&**txt)?.to_string()));
                }
                _ => {}
            },
            Event::Start(_) => self.depth += 1,
            Event::End(_) => {
                match current {
                    CurrentTag::RungEntity => {
                        let main_comment = std::mem::replace(&mut self.new_comment, Vec::new());
                        let name = self.mk_rung_name();
                        self.rungs.push(Rung { name, main_comment });
                    }
                    CurrentTag::GrafcetNodeStep => {
                        let name = self
                            .names
                            .iter()
                            .find(|&&(depth, _)| depth > self.depth)
                            .map_or_else(String::new, |(_, name)| name.clone());
                        self.ids.insert(self.new_id.clone(), name);
                    }
                    CurrentTag::GrafcetTransition => {
                        let name = self.latest_name();
                        self.ids.insert(self.new_id.clone(), name);
                        self.remove_old_names();
                    }
                    _ => {}
                }
                self.depth -= 1;
            }
            _ => {}
        }
        Ok(VisitProcessing::Continue(event))
    }
}

#[derive(Debug)]
struct DiffHeader<'a> {
    trk: &'a NameTracker,
    grc: &'a GrafcetTracer,
    grc_cnt: GrafcetCounter,
    current_rung: usize,
}
impl<'a> DiffHeader<'a> {
    pub fn new(trk: &'a NameTracker, grc: &'a GrafcetTracer) -> Self {
        Self {
            trk,
            grc,
            grc_cnt: Default::default(),
            current_rung: 0,
        }
    }
    fn add_ctx_attr(bytes: &mut BytesStart, hdr: &dyn AsRef<[u8]>) {
        bytes.push_attribute((&b"ctx"[..], hdr.as_ref()));
    }
    fn id(&self, id: &'a Guid) -> &'a str {
        if let Some(name) = &self.trk.ids.get(id) {
            name
        } else {
            let x = self.grc.get_unique_link(id);
            self.id(x)
        }
    }
    fn trans_ctx(&self) -> Vec<u8> {
        let node = self.grc.get_current_node(&self.grc_cnt);
        let (from, id, to) = node.uniq_triple().expect("Failed to get uniq triple");
        format!("{}->[{}]->{}", self.id(from), self.id(id), self.id(to)).into()
    }
}
impl XmlNodeVisitor for DiffHeader<'_> {
    fn visit<'a>(&mut self, mut event: Event<'a>, current: CurrentTag) -> VisitResult<'a> {
        if let Event::Start(bytes) = &mut event {
            self.grc_cnt.process_current_tag(current);
            match current {
                CurrentTag::RungEntity => {
                    Self::add_ctx_attr(bytes, &self.trk.rungs[self.current_rung].name);
                    self.current_rung += 1;
                }
                CurrentTag::GrafcetTransition => {
                    Self::add_ctx_attr(bytes, &self.trans_ctx());
                }
                _ => {}
            }
        }
        Ok(VisitProcessing::Continue(event))
    }
}

fn output_visitor(filename: &Path) -> Result<()> {
    let mut ionames = IoNames::new();
    let mut name_tracker = NameTracker::default();
    let mut grafcet_tracer = GrafcetTracer::default();
    process_file(
        filename,
        &mut [
            &mut ionames,        // Collect symbols for IO addresses
            &mut name_tracker,   // Collect context for diff headers
            &mut grafcet_tracer, // Check the Grafcet node connections
        ],
    )
    .context("Pre-processing failed")?;

    // let out = BufWriter::new(File::create("out.xml")?);
    // let out = std::io::sink();
    let out = std::io::stdout();

    let mut guid_map = GuidVisitor::new();
    let mut writer = EventWriter(Writer::new(out));
    let mut tag_skipper = SkipTag::new(CurrentTag::LadderElements);
    let mut inst_line_mangle = NormalizeInstructionLine::new(&ionames);
    let mut diff_headers = DiffHeader::new(&name_tracker, &grafcet_tracer);
    process_file(
        filename,
        &mut [
            &mut tag_skipper,      // skip ladder diagram tags
            &mut diff_headers,     // Generate diff headers
            &mut inst_line_mangle, // Mangle instruction lines
            &mut guid_map,         // map GUID
            &mut writer,           // write output
        ],
    )
    .context("Post-processing failed")
}

fn main() -> Result<()> {
    let filename = env::args()
        .nth(1)
        .context("Missing filename on commandline")?;
    output_visitor(Path::new(&*filename))
}

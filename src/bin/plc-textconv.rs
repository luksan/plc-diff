use anyhow::{Context, Result};
use quick_xml::events::{BytesText, Event};
use quick_xml::Writer;

use std::env;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::BufWriter;
use std::path::Path;

use plc_diff::{process_file, CurrentTag, GuidMap, VisitProcessing, VisitResult, XmlNodeVisitor};

#[derive(Debug, Default)]
struct NormalizeInstructionLine {
    in_entity: bool,
    text: Vec<u8>,
}

impl NormalizeInstructionLine {
    fn new() -> NormalizeInstructionLine {
        Self {
            in_entity: false,
            text: Vec::new(),
        }
    }
}

impl XmlNodeVisitor for NormalizeInstructionLine {
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
                let mut new = normalize_whitespace(txt);
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

fn output_visitor(filename: &Path) -> Result<()> {
    // let out = BufWriter::new(File::create("out.xml")?);
    // let out = std::io::sink();
    let out = std::io::stdout();

    let mut guid_map = GuidVisitor::new();
    let mut writer = EventWriter(Writer::new(out));
    let mut tag_skipper = SkipTag::new(CurrentTag::LadderElements);
    let mut inst_line_mangle = NormalizeInstructionLine::new();
    process_file(
        filename,
        &mut [
            &mut tag_skipper,      // skip ladder diagram tags
            &mut guid_map,         // map GUID
            &mut inst_line_mangle, // Normalize whitespace
            &mut writer,           // write output
        ],
    )
}

fn normalize_whitespace(txt: &BytesText) -> Vec<u8> {
    let mut new = Vec::new();
    for word in (*txt).split(|c| c.is_ascii_whitespace()) {
        if word.is_empty() {
            continue;
        }
        new.extend_from_slice(word);
        new.push(b' ');
    }
    new.pop();
    new
}

fn main() -> Result<()> {
    let filename = env::args()
        .nth(1)
        .context("Missing filename on commandline")?;
    output_visitor(Path::new(&*filename))
}

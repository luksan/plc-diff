use anyhow::{Context, Result};
use quick_xml::events::{BytesText, Event};
use quick_xml::Writer;

use std::env;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::BufWriter;
use std::path::Path;

use plc_diff::{process_file, CurrentTag, GuidMap, VisitProcessing};

#[derive(Debug, Default)]
struct NormalizeInstructionLine {}
impl NormalizeInstructionLine {
    fn new() -> NormalizeInstructionLine {
        Self {}
    }
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> Result<VisitProcessing<'a>> {
        match &event {
            Event::Text(txt) if current == CurrentTag::InstructionLine => {
                return Ok(VisitProcessing::Continue(normalize_whitespace(txt)))
            }
            _ => {}
        }
        Ok(VisitProcessing::Continue(event))
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

fn output_visitor(filename: &Path) -> Result<()> {
    // let out = BufWriter::new(File::create("out.xml")?);
    // let out = std::io::sink();
    let out = std::io::stdout();

    let mut guid_map = GuidVisitor::new();
    let mut writer = Writer::new(out);
    let mut tag_skipper = SkipTag::new(CurrentTag::LadderElements);
    let mut inst_line_mangle = NormalizeInstructionLine::new();
    process_file(
        filename,
        &mut [
            &mut |ev, tag| tag_skipper.visit(ev, tag),      // skip tag
            &mut |ev, tag| guid_map.visit(ev, tag),         // map GUID
            &mut |ev, tag| inst_line_mangle.visit(ev, tag), // Normalize whitespace
            &mut |ev, _| {
                writer.write_event(ev)?;
                Ok(VisitProcessing::NextNode)
            }, //write output
        ],
    )
}

fn normalize_whitespace(txt: &BytesText) -> Event<'static> {
    let mut new = Vec::new();
    for word in (*txt).split(|c| c.is_ascii_whitespace()) {
        if word.is_empty() {
            continue;
        }
        new.extend_from_slice(word);
        new.push(b' ');
    }
    new.pop();
    Event::Text(BytesText::from_escaped(new))
}

fn main() -> Result<()> {
    let filename = env::args()
        .nth(1)
        .context("Missing filename on commandline")?;
    output_visitor(Path::new(&*filename))
}

use anyhow::{Context, Result};
use quick_xml::events::{BytesText, Event};
use quick_xml::{Reader, Writer};

use std::env;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::BufWriter;
use std::path::Path;

use plc_diff::GuidMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum CurrentTag {
    Id,
    To,
    From,
    InstructionLine,
    LadderElements,
    Other,
    None,
}

impl From<&[u8]> for CurrentTag {
    fn from(tag: &[u8]) -> Self {
        match tag {
            b"Id" => Self::Id,
            b"From" => Self::From,
            b"To" => Self::To,
            b"InstructionLine" => Self::InstructionLine,
            b"LadderElements" => Self::LadderElements,
            _ => Self::Other,
        }
    }
}

fn load_xml(filename: &Path) -> Result<()> {
    let mut reader = Reader::from_file(filename)?;

    // let out = BufWriter::new(File::create("out.xml")?);
    // let out = std::io::sink();
    let out = std::io::stdout();

    let mut writer = Writer::new(out);

    let mut current_tag = CurrentTag::None;
    let mut id_map = GuidMap::new();
    let mut read_buf = Vec::new();
    let mut skip_tag = None;
    loop {
        let ev = reader.read_event(&mut read_buf)?;
        let new = match &ev {
            Event::Start(st) if skip_tag.is_none() => {
                current_tag = st.local_name().into();
                if matches!(current_tag, CurrentTag::LadderElements) {
                    skip_tag = Some(current_tag);
                }
                ev
            }
            Event::End(end_tag) => {
                let closes = CurrentTag::from(end_tag.local_name());
                current_tag = CurrentTag::None;
                if skip_tag.is_some() {
                    if Some(closes) == skip_tag {
                        skip_tag = None;
                    } else {
                        continue;
                    }
                }
                ev
            }
            Event::Text(txt) if skip_tag.is_none() => match current_tag {
                CurrentTag::Id | CurrentTag::To | CurrentTag::From => {
                    let new = id_map.get_or_insert(txt)?;
                    Event::Text(BytesText::from_escaped_str(format!("=={}==", new)))
                }
                CurrentTag::InstructionLine => {
                    // normalize whitespace
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
                _ => ev,
            },
            Event::Eof => break,
            _ if skip_tag.is_some() => continue,
            _ => ev,
        };
        writer.write_event(new)?;
        read_buf.clear();
    }
    Ok(())
}

fn main() -> Result<()> {
    let filename = env::args()
        .nth(1)
        .context("Missing filename on commandline")?;
    load_xml(Path::new(&*filename))
}

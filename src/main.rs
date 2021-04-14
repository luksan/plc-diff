use anyhow::{Context, Result};
use quick_xml::events::{BytesStart, BytesText, Event};
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
    Other,
    None,
}

impl From<&BytesStart<'_>> for CurrentTag {
    fn from(tag: &BytesStart) -> Self {
        match tag.local_name() {
            b"Id" => Self::Id,
            b"From" => Self::From,
            b"To" => Self::To,
            b"InstructionLine" => Self::InstructionLine,
            _ => Self::Other,
        }
    }
}

fn load_xml(filename: &Path) -> Result<()> {
    let mut reader = Reader::from_file(filename)?;

    // let out = BufWriter::new(File::create("out.xml")?);
    let out = std::io::stdout();

    let mut writer = Writer::new(out);

    let mut current_tag = CurrentTag::None;
    let mut id_map = GuidMap::new();
    let mut read_buf = Vec::new();
    loop {
        let ev = reader.read_event(&mut read_buf)?;
        let new = match &ev {
            Event::Start(st) => {
                current_tag = st.into();
                ev
            }
            Event::End(_) => {
                current_tag = CurrentTag::None;
                ev
            }
            Event::Text(txt) => match current_tag {
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

use anyhow::{Context, Result};
use quick_xml::events::{BytesText, Event};
use quick_xml::{Reader, Writer};

use std::collections::HashMap;
use std::env;
#[allow(unused_imports)]
use std::fs::File;
use std::hash::{Hash, Hasher};
#[allow(unused_imports)]
use std::io::BufWriter;
use std::path::Path;

#[derive(Debug, Clone)]
struct Txt(BytesText<'static>);

impl PartialEq for Txt {
    fn eq(&self, other: &Self) -> bool {
        *self.0 == *other.0
    }
}
impl Eq for Txt {}

impl Hash for Txt {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl From<BytesText<'static>> for Txt {
    fn from(txt: BytesText<'static>) -> Self {
        Self(txt)
    }
}

fn load_xml(filename: &Path) -> Result<()> {
    let mut reader = Reader::from_file(filename)?;

    // let out = BufWriter::new(File::create("out.xml")?);
    let out = std::io::stdout();

    let mut writer = Writer::new(out);

    let mut id = false;
    let mut id_count = 0;
    let mut id_map = HashMap::<Txt, u32>::new();
    let mut read_buf = Vec::new();
    loop {
        let ev = reader.read_event(&mut read_buf)?;
        let new = match &ev {
            Event::Start(st) => {
                id = st.local_name() == b"Id"
                    || st.local_name() == b"To"
                    || st.local_name() == b"From";
                ev
            }
            Event::End(_) => {
                id = false;
                ev
            }
            Event::Text(txt) => {
                if id {
                    let new = id_map
                        .entry(txt.clone().into_owned().into())
                        .or_insert_with(|| {
                            id_count += 1;
                            id_count
                        });
                    Event::Text(BytesText::from_escaped_str(format!("=={}==", new)))
                } else {
                    ev
                }
            }
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

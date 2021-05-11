use std::collections::HashMap;
use std::convert::TryInto;
use std::mem::take;

use anyhow::bail;
use quick_xml::events::Event;

use crate::{CurrentTag, Guid, VisitProcessing, VisitResult, XmlNodeVisitor};

#[derive(Debug, Default)]
pub struct GrafcetNode {
    pub id: Guid,
    pub from: Vec<Guid>,
    pub to: Vec<Guid>,
}
impl GrafcetNode {
    /// Return Some((from, id, to)) Guid links if from and to are unique
    pub fn uniq_triple(&self) -> Option<(&Guid, &Guid, &Guid)> {
        if self.from.len() == 1 && self.to.len() == 1 {
            Some((&self.from[0], &self.id, &self.to[0]))
        } else {
            None
        }
    }
}

#[derive(Debug, Default)]
pub struct GrafcetCounter(usize);

impl GrafcetCounter {
    pub fn process_current_tag(&mut self, current: CurrentTag) -> bool {
        match current {
            CurrentTag::GrafcetNodeStep
            | CurrentTag::GrafcetTransition
            | CurrentTag::GrafcetOrFork
            | CurrentTag::GrafcetOrJunction => {
                self.0 += 1;
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Default)]
pub struct GrafcetTracer {
    nodes: HashMap<Guid, GrafcetNode>,
    sequence: Vec<Guid>,
    counter: GrafcetCounter,
    new_node: (usize, GrafcetNode),
    current_depth: usize,
}

impl GrafcetTracer {
    pub fn get_unique_link(&self, id: &Guid) -> &Guid {
        let curr = &self.nodes[id];
        assert!(curr.to.len() == 1 || curr.from.len() == 1);
        if curr.to.len() == 1 {
            &curr.to[0]
        } else {
            &curr.from[0]
        }
    }
    pub fn get_current_node(&self, cnt: &GrafcetCounter) -> &GrafcetNode {
        &self.nodes[&self.sequence[cnt.0 - 1]]
    }
}

impl XmlNodeVisitor for GrafcetTracer {
    fn visit<'a>(&mut self, event: Event<'a>, current: CurrentTag) -> VisitResult<'a> {
        match &event {
            Event::Text(txt) => match current {
                CurrentTag::Id => {
                    self.new_node.0 = self.current_depth;
                    self.new_node.1.id = txt.try_into()?
                }
                CurrentTag::To => self.new_node.1.to.push(txt.try_into()?),
                CurrentTag::From => self.new_node.1.from.push(txt.try_into()?),
                _ => {}
            },
            Event::Start(_) => self.current_depth += 1,
            Event::End(_) => {
                if self.current_depth + 1 < self.new_node.0 {
                    bail!("Failed to generate grafcet trace {:?}", self.new_node);
                }
                if self.counter.process_current_tag(current) {
                    assert!(
                        (self.new_node.1.from.len() == 1) || (self.new_node.1.to.len() == 1),
                        "{:?}",
                        self.new_node
                    );
                    let (_depth, node) = take(&mut self.new_node);
                    self.sequence.push(node.id.clone());
                    self.nodes.insert(node.id.clone(), node);
                }
                self.current_depth -= 1;
            }
            _ => {}
        }
        Ok(VisitProcessing::Continue(event))
    }
}

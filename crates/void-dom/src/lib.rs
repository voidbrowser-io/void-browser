//! Void DOM: a flat arena of nodes. No legacy APIs — just what a signals
//! runtime needs to build and patch a UI.

use std::collections::{HashMap, HashSet};

pub type NodeId = u32;
pub const ROOT: NodeId = 0;

pub enum NodeKind {
    Element {
        tag: String,
        attrs: HashMap<String, String>,
        classes: Vec<String>,
    },
    Text(String),
}

pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub inline_style: String,
    pub listeners: HashSet<String>,
    /// Value of an `<input>`.
    pub value: String,
    /// The whole input value is selected (typing replaces it).
    pub selected: bool,
}

impl Node {
    pub fn tag(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Element { tag, .. } => Some(tag),
            NodeKind::Text(_) => None,
        }
    }
}

pub struct Dom {
    pub nodes: Vec<Node>,
    pub dirty: bool,
    pub hover: Option<NodeId>,
    pub active: Option<NodeId>,
    pub focus: Option<NodeId>,
}

impl Dom {
    pub fn new() -> Self {
        let mut dom = Dom { nodes: Vec::new(), dirty: true, hover: None, active: None, focus: None };
        dom.create_element("void-root");
        dom
    }

    fn push(&mut self, kind: NodeKind) -> NodeId {
        self.nodes.push(Node {
            kind,
            parent: None,
            children: Vec::new(),
            inline_style: String::new(),
            listeners: HashSet::new(),
            value: String::new(),
            selected: false,
        });
        (self.nodes.len() - 1) as NodeId
    }

    pub fn create_element(&mut self, tag: &str) -> NodeId {
        self.push(NodeKind::Element { tag: tag.to_ascii_lowercase(), attrs: HashMap::new(), classes: Vec::new() })
    }

    pub fn create_text(&mut self, text: &str) -> NodeId {
        self.push(NodeKind::Text(text.to_string()))
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn set_text(&mut self, id: NodeId, text: &str) {
        if let Some(Node { kind: NodeKind::Text(t), .. }) = self.nodes.get_mut(id as usize) {
            if t != text {
                *t = text.to_string();
                self.dirty = true;
            }
        }
    }

    pub fn remove(&mut self, child: NodeId) {
        let Some(parent) = self.nodes.get(child as usize).and_then(|n| n.parent) else { return };
        self.nodes[parent as usize].children.retain(|&c| c != child);
        self.nodes[child as usize].parent = None;
        if self.focus.is_some_and(|f| self.is_ancestor(child, f)) {
            self.focus = None;
        }
        self.dirty = true;
    }

    /// Insert `child` into `parent` before `before` (or at the end).
    pub fn insert(&mut self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        if parent as usize >= self.nodes.len() || child as usize >= self.nodes.len() || parent == child {
            return;
        }
        self.remove(child);
        let children = &mut self.nodes[parent as usize].children;
        let at = before.and_then(|b| children.iter().position(|&c| c == b)).unwrap_or(children.len());
        children.insert(at, child);
        self.nodes[child as usize].parent = Some(parent);
        self.dirty = true;
    }

    /// Detach every sibling strictly between `start` and `end`.
    pub fn clear_between(&mut self, start: NodeId, end: NodeId) {
        let Some(parent) = self.get(start).and_then(|n| n.parent) else { return };
        let children = &self.nodes[parent as usize].children;
        let (Some(a), Some(b)) = (children.iter().position(|&c| c == start), children.iter().position(|&c| c == end))
        else {
            return;
        };
        let doomed: Vec<NodeId> = children[a + 1..b].to_vec();
        for c in doomed {
            self.remove(c);
        }
    }

    pub fn set_attr(&mut self, id: NodeId, name: &str, value: &str) {
        let Some(node) = self.nodes.get_mut(id as usize) else { return };
        match name {
            "style" => node.inline_style = value.to_string(),
            "value" => node.value = value.to_string(),
            _ => {
                if let NodeKind::Element { attrs, classes, .. } = &mut node.kind {
                    if name == "class" {
                        *classes = value.split_whitespace().map(str::to_string).collect();
                    }
                    attrs.insert(name.to_string(), value.to_string());
                }
            }
        }
        self.dirty = true;
    }

    /// Focus an `<input>` and select its value, like clicking an address bar.
    pub fn focus_input(&mut self, id: NodeId) {
        if self.get(id).and_then(|n| n.tag()) == Some("input") {
            if let Some(old) = self.focus {
                self.nodes[old as usize].selected = false;
            }
            self.focus = Some(id);
            self.nodes[id as usize].selected = true;
            self.dirty = true;
        }
    }

    pub fn listen(&mut self, id: NodeId, event: &str) {
        if let Some(node) = self.nodes.get_mut(id as usize) {
            node.listeners.insert(event.to_string());
        }
    }

    pub fn is_ancestor(&self, ancestor: NodeId, mut node: NodeId) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            match self.get(node).and_then(|n| n.parent) {
                Some(p) => node = p,
                None => return false,
            }
        }
    }

    /// Event path from `target` up to the root.
    pub fn path(&self, target: NodeId) -> Vec<NodeId> {
        let mut path = vec![target];
        let mut cur = target;
        while let Some(p) = self.get(cur).and_then(|n| n.parent) {
            path.push(p);
            cur = p;
        }
        path
    }
}

//! A small mutable arena DOM, mirroring the subset of `golang.org/x/net/html`
//! and `JohannesKaufmann/dom` that html-to-markdown relies on.

use markup5ever_rcdom::{Handle, NodeData};

pub type Id = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Document,
    Element,
    Text,
    Comment,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    /// Tag name for elements, text for text/comment nodes.
    pub data: String,
    pub attrs: Vec<(String, String)>,
    pub parent: Option<Id>,
    pub first_child: Option<Id>,
    pub last_child: Option<Id>,
    pub prev: Option<Id>,
    pub next: Option<Id>,
}

pub struct Dom {
    pub nodes: Vec<Node>,
}

pub const ROOT: Id = 0;

impl Dom {
    pub fn parse(html: &str) -> Dom {
        let tree = htmd::HtmlToMarkdown::new()
            .html_to_tree(html)
            .expect("html parsing never fails on a string");
        let mut dom = Dom { nodes: Vec::new() };
        dom.new_node(Kind::Document, String::new());
        dom.import_children(&tree, ROOT);
        dom
    }

    fn import_children(&mut self, handle: &Handle, parent: Id) {
        for child in handle.children.borrow().iter() {
            let id = match &child.data {
                NodeData::Element { name, attrs, .. } => {
                    let id = self.new_node(Kind::Element, name.local.to_string());
                    self.nodes[id].attrs = attrs
                        .borrow()
                        .iter()
                        .map(|a| (a.name.local.to_string(), a.value.to_string()))
                        .collect();
                    id
                }
                NodeData::Text { contents } => {
                    self.new_node(Kind::Text, contents.borrow().to_string())
                }
                NodeData::Comment { contents } => self.new_node(Kind::Comment, contents.to_string()),
                _ => continue,
            };
            self.append_child(parent, id);
            self.import_children(child, id);
        }
    }

    pub fn new_node(&mut self, kind: Kind, data: String) -> Id {
        self.nodes.push(Node {
            kind,
            data,
            attrs: Vec::new(),
            parent: None,
            first_child: None,
            last_child: None,
            prev: None,
            next: None,
        });
        self.nodes.len() - 1
    }

    pub fn kind(&self, n: Id) -> Kind {
        self.nodes[n].kind
    }

    pub fn data(&self, n: Id) -> &str {
        &self.nodes[n].data
    }

    pub fn set_data(&mut self, n: Id, data: impl Into<String>) {
        self.nodes[n].data = data.into();
    }

    pub fn parent(&self, n: Id) -> Option<Id> {
        self.nodes[n].parent
    }
    pub fn first_child(&self, n: Id) -> Option<Id> {
        self.nodes[n].first_child
    }
    pub fn last_child(&self, n: Id) -> Option<Id> {
        self.nodes[n].last_child
    }
    pub fn next(&self, n: Id) -> Option<Id> {
        self.nodes[n].next
    }
    pub fn prev(&self, n: Id) -> Option<Id> {
        self.nodes[n].prev
    }

    /// Equivalent of `dom.NodeName`.
    pub fn name(&self, n: Id) -> &str {
        match self.nodes[n].kind {
            Kind::Document => "#document",
            Kind::Text => "#text",
            Kind::Comment => "#comment",
            Kind::Element => &self.nodes[n].data,
        }
    }

    pub fn attr(&self, n: Id, key: &str) -> Option<&str> {
        self.nodes[n]
            .attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn attr_or<'a>(&'a self, n: Id, key: &str, fallback: &'a str) -> &'a str {
        self.attr(n, key).unwrap_or(fallback)
    }

    pub fn children(&self, n: Id) -> Vec<Id> {
        let mut out = Vec::new();
        let mut c = self.first_child(n);
        while let Some(id) = c {
            out.push(id);
            c = self.next(id);
        }
        out
    }

    pub fn first_child_element(&self, n: Id) -> Option<Id> {
        self.children(n).into_iter().find(|&c| self.kind(c) == Kind::Element)
    }

    pub fn next_sibling_element(&self, n: Id) -> Option<Id> {
        let mut s = self.next(n);
        while let Some(id) = s {
            if self.kind(id) == Kind::Element {
                return Some(id);
            }
            s = self.next(id);
        }
        None
    }

    pub fn all_nodes(&self, start: Id) -> Vec<Id> {
        let mut out = Vec::new();
        self.collect_all(start, &mut out);
        out
    }

    fn collect_all(&self, n: Id, out: &mut Vec<Id>) {
        out.push(n);
        let mut c = self.first_child(n);
        while let Some(id) = c {
            self.collect_all(id, out);
            c = self.next(id);
        }
    }

    pub fn collect_text(&self, n: Id) -> String {
        let mut s = String::new();
        for id in self.all_nodes(n) {
            if self.kind(id) == Kind::Text {
                s.push_str(self.data(id));
            }
        }
        s
    }

    // ----- mutation -----

    pub fn remove(&mut self, n: Id) {
        let Some(p) = self.nodes[n].parent else { return };
        let prev = self.nodes[n].prev;
        let next = self.nodes[n].next;
        match prev {
            Some(pr) => self.nodes[pr].next = next,
            None => self.nodes[p].first_child = next,
        }
        match next {
            Some(nx) => self.nodes[nx].prev = prev,
            None => self.nodes[p].last_child = prev,
        }
        self.nodes[n].parent = None;
        self.nodes[n].prev = None;
        self.nodes[n].next = None;
    }

    pub fn append_child(&mut self, p: Id, c: Id) {
        self.remove(c);
        let last = self.nodes[p].last_child;
        self.nodes[c].parent = Some(p);
        self.nodes[c].prev = last;
        match last {
            Some(l) => self.nodes[l].next = Some(c),
            None => self.nodes[p].first_child = Some(c),
        }
        self.nodes[p].last_child = Some(c);
    }

    /// Insert `c` into `p` before `before` (or at the end when `None`).
    pub fn insert_before(&mut self, p: Id, c: Id, before: Option<Id>) {
        let Some(b) = before else {
            self.append_child(p, c);
            return;
        };
        self.remove(c);
        let prev = self.nodes[b].prev;
        self.nodes[c].parent = Some(p);
        self.nodes[c].prev = prev;
        self.nodes[c].next = Some(b);
        self.nodes[b].prev = Some(c);
        match prev {
            Some(pr) => self.nodes[pr].next = Some(c),
            None => self.nodes[p].first_child = Some(c),
        }
    }

    pub fn unwrap_node(&mut self, n: Id) {
        let Some(p) = self.parent(n) else { return };
        while let Some(c) = self.first_child(n) {
            self.insert_before(p, c, Some(n));
        }
        self.remove(n);
    }

    pub fn wrap_node(&mut self, existing: Id, wrapper: Id) -> Id {
        let Some(p) = self.parent(existing) else { return existing };
        self.insert_before(p, wrapper, Some(existing));
        self.append_child(wrapper, existing);
        wrapper
    }

    /// Replace all children with a single text node (goquery `SetText`).
    pub fn set_text(&mut self, n: Id, text: String) {
        while let Some(c) = self.first_child(n) {
            self.remove(c);
        }
        let t = self.new_node(Kind::Text, text);
        self.append_child(n, t);
    }

    pub fn swap_tags(&mut self, a: Id, b: Id) {
        let (da, aa) = (self.nodes[a].data.clone(), self.nodes[a].attrs.clone());
        self.nodes[a].data = self.nodes[b].data.clone();
        self.nodes[a].attrs = self.nodes[b].attrs.clone();
        self.nodes[b].data = da;
        self.nodes[b].attrs = aa;
    }

    // ----- neighbor traversal (dom/neighbors.go) -----

    fn neighbor(
        &self,
        node: Id,
        first: impl Fn(&Self, Id) -> Option<Id>,
        sibling: impl Fn(&Self, Id) -> Option<Id>,
        stop_at: Option<Id>,
    ) -> Option<Id> {
        if let Some(c) = first(self, node) {
            return Some(c);
        }
        if let Some(s) = sibling(self, node) {
            return Some(s);
        }
        let mut n = node;
        loop {
            n = self.parent(n)?;
            if Some(n) == stop_at {
                return None;
            }
            if let Some(s) = sibling(self, n) {
                return Some(s);
            }
        }
    }

    pub fn next_neighbor_node(&self, n: Id) -> Option<Id> {
        self.neighbor(n, Self::first_child, Self::next, None)
    }
    pub fn prev_neighbor_node(&self, n: Id) -> Option<Id> {
        self.neighbor(n, Self::first_child, Self::prev, None)
    }
    pub fn next_neighbor_element(&self, n: Id) -> Option<Id> {
        self.neighbor(n, Self::first_child_element, Self::next_sibling_element, None)
    }
    pub fn next_neighbor_node_excluding_own_child(&self, n: Id) -> Option<Id> {
        self.neighbor(n, |_, _| None, Self::next, None)
    }
    pub fn prev_neighbor_node_excluding_own_child(&self, n: Id) -> Option<Id> {
        self.neighbor(n, |_, _| None, Self::prev, None)
    }

    /// `dom.FindFirstNode`: depth-first search below `start`.
    pub fn find_first(&self, start: Id, f: impl Fn(&Self, Id) -> bool) -> Option<Id> {
        self.find_all(start, f).into_iter().next()
    }

    pub fn find_all(&self, start: Id, f: impl Fn(&Self, Id) -> bool) -> Vec<Id> {
        let mut out = Vec::new();
        let mut child = self.first_child(start);
        while let Some(c) = child {
            if f(self, c) {
                out.push(c);
            }
            child = self.neighbor(c, Self::first_child, Self::next, Some(start));
        }
        out
    }
}

pub fn name_is_block_node(name: &str) -> bool {
    matches!(
        name,
        "address" | "article" | "aside" | "blockquote" | "details" | "dialog" | "dd" | "div"
            | "dl" | "dt" | "fieldset" | "figcaption" | "figure" | "footer" | "form" | "h1"
            | "h2" | "h3" | "h4" | "h5" | "h6" | "header" | "hgroup" | "hr" | "li" | "main"
            | "nav" | "ol" | "p" | "pre" | "section" | "table" | "ul"
    )
}

pub fn name_is_inline_node(name: &str) -> bool {
    matches!(
        name,
        "#text" | "a" | "abbr" | "acronym" | "audio" | "b" | "bdi" | "bdo" | "big" | "br"
            | "button" | "canvas" | "cite" | "code" | "data" | "datalist" | "del" | "dfn"
            | "em" | "embed" | "i" | "iframe" | "img" | "input" | "ins" | "kbd" | "label"
            | "map" | "mark" | "meter" | "noscript" | "object" | "output" | "picture"
            | "progress" | "q" | "ruby" | "s" | "samp" | "script" | "select" | "slot"
            | "small" | "span" | "strong" | "sub" | "sup" | "svg" | "template" | "textarea"
            | "time" | "u" | "tt" | "var" | "video" | "wbr"
    )
}

pub fn name_is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

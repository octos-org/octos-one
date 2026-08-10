//! The node tree and its wire format.
//!
//! Byte-compatible with `Node.decode` on the Java side. The format is deliberately
//! flat: a fixed-stride record section plus one string blob, so Java walks it without
//! allocating per node and the whole tree crosses JNI in ONE call.
//!
//! ```text
//! magic:u32 = 0x53504332   count:u32   blob_len:u32
//! count × { id:u32  parent:u32  kind:str32  attr_count:u32
//!           attr_count × { key:str32  tag:u32(0=f64,1=str)  value } }
//! blob
//! ```
//!
//! The tag is FOUR bytes, not one — the reader takes the tag then three padding bytes,
//! which keeps an f64 8-byte aligned. Writing a bare byte desynchronises every
//! attribute after the first, producing a corrupt view tree rather than a clean
//! failure. Checked against the decoder, not assumed.

pub const MAGIC: u32 = 0x5350_4332;
const T_F64: u8 = 0;
const T_STR: u8 = 1;

#[derive(Debug, Clone)]
pub enum Val {
    F(f64),
    S(String),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: String,
    pub attrs: Vec<(String, Val)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(kind: &str) -> Self {
        Node { kind: kind.to_string(), attrs: Vec::new(), children: Vec::new() }
    }
    pub fn s(mut self, k: &str, v: &str) -> Self {
        self.attrs.push((k.to_string(), Val::S(v.to_string())));
        self
    }
    pub fn n(mut self, k: &str, v: f64) -> Self {
        self.attrs.push((k.to_string(), Val::F(v)));
        self
    }
    pub fn kids(mut self, c: Vec<Node>) -> Self {
        self.children.extend(c);
        self
    }
}

pub fn encode(root: &Node) -> Vec<u8> {
    struct Enc { rec: Vec<u8>, blob: Vec<u8>, count: u32, next: u32 }
    impl Enc {
        fn str32(&mut self, s: &str) -> (u32, u32) {
            let o = self.blob.len() as u32;
            self.blob.extend_from_slice(s.as_bytes());
            (o, s.len() as u32)
        }
        fn walk(&mut self, n: &Node, parent: u32) {
            let id = self.next;
            self.next += 1;
            self.count += 1;
            let (ko, kl) = self.str32(&n.kind);
            self.rec.extend_from_slice(&id.to_le_bytes());
            self.rec.extend_from_slice(&parent.to_le_bytes());
            self.rec.extend_from_slice(&ko.to_le_bytes());
            self.rec.extend_from_slice(&kl.to_le_bytes());
            self.rec.extend_from_slice(&(n.attrs.len() as u32).to_le_bytes());
            for (k, v) in &n.attrs {
                let (o, l) = self.str32(k);
                self.rec.extend_from_slice(&o.to_le_bytes());
                self.rec.extend_from_slice(&l.to_le_bytes());
                match v {
                    Val::F(f) => {
                        self.rec.extend_from_slice(&[T_F64, 0, 0, 0]);
                        self.rec.extend_from_slice(&f.to_le_bytes());
                    }
                    Val::S(s) => {
                        self.rec.extend_from_slice(&[T_STR, 0, 0, 0]);
                        let (o, l) = self.str32(s);
                        self.rec.extend_from_slice(&o.to_le_bytes());
                        self.rec.extend_from_slice(&l.to_le_bytes());
                    }
                }
            }
            for c in &n.children { self.walk(c, id); }
        }
    }
    let mut e = Enc { rec: Vec::new(), blob: Vec::new(), count: 0, next: 0 };
    e.walk(root, u32::MAX);
    let mut out = Vec::with_capacity(12 + e.rec.len() + e.blob.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&e.count.to_le_bytes());
    out.extend_from_slice(&(e.blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&e.rec);
    out.extend_from_slice(&e.blob);
    out
}

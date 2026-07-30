//! Plans → the Splash-Android node wire format.
//!
//! octos-one's other lowering ([`super::weather`] and friends) emits makepad Splash
//! DSL. This one emits the **node tree** that `ymote/Splash-Android` decodes into
//! `android.widget.*` / `com.google.android.material.*` views — so the same plan can be
//! rendered by makepad's GPU renderer or by native Android views, chosen at runtime.
//!
//! ## Why a second lowering rather than a shared tree
//!
//! An earlier design tried to put one backend-agnostic node tree UNDER the existing
//! makepad dialect. That fails: card evaluation ends after makepad's widget
//! constructors have resolved through its registry, `ui.<id>` is a live handle into the
//! real widget tree, and shader uniforms are a GPU ABI rather than a node property.
//!
//! Lowering a typed PLAN twice is a different proposition — two `match` statements over
//! the same closed vocabulary. Neither knows about the other, and adding a backend
//! cannot break an existing one.
//!
//! ## The wire format
//!
//! Little-endian, one buffer per render, decoded by `Node.decode` on the Java side:
//!
//! ```text
//! magic:u32 = 0x53504332   count:u32   blob_len:u32
//! count × { id:u32  parent:u32  kind:str32  attr_count:u32
//!           attr_count × { key:str32  tag:u32(0=f64,1=str)  value } }
//! blob (all string bytes, referenced as offset+len into it)
//! ```
//!
//! `str32` is an offset+len pair into the blob, so the record section is fixed-stride
//! and Java can walk it without allocating per node.
//!
//! ## What this file does NOT do
//!
//! Resolve data. The node tree carries `sys.*` CALL TEXT in the same places the DSL
//! lowering does, and the Android side evaluates it — mirroring how the makepad card
//! defers to the runtime rather than baking values in. That keeps one rule true on both
//! backends: a card never contains a value the model supplied.

use super::common::locale_tag;

/// Wire-format magic. Must match `Node.MAGIC` on the Java side; a mismatch means the
/// two halves were built from different revisions, and it is better to fail loudly at
/// the first four bytes than to decode garbage into a view tree.
pub const MAGIC: u32 = 0x5350_4332;

const T_F64: u8 = 0;
const T_STR: u8 = 1;

/// One node: a kind, flat attributes, and children.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub kind: String,
    pub attrs: Vec<(String, Val)>,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    F(f64),
    S(String),
}

impl Node {
    pub fn new(kind: &str) -> Self {
        Node {
            kind: kind.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }
    pub fn s(mut self, k: &str, v: &str) -> Self {
        self.attrs.push((k.to_string(), Val::S(v.to_string())));
        self
    }
    pub fn n(mut self, k: &str, v: f64) -> Self {
        self.attrs.push((k.to_string(), Val::F(v)));
        self
    }
    pub fn kid(mut self, c: Node) -> Self {
        self.children.push(c);
        self
    }
    pub fn kids(mut self, c: Vec<Node>) -> Self {
        self.children.extend(c);
        self
    }
}

/// Serialize a tree to the wire format.
pub fn encode(root: &Node) -> Vec<u8> {
    struct Enc {
        rec: Vec<u8>,
        blob: Vec<u8>,
        count: u32,
        next_id: u32,
    }
    impl Enc {
        fn str32(&mut self, s: &str) -> (u32, u32) {
            let off = self.blob.len() as u32;
            self.blob.extend_from_slice(s.as_bytes());
            (off, s.len() as u32)
        }
        fn walk(&mut self, n: &Node, parent: u32) {
            let id = self.next_id;
            self.next_id += 1;
            self.count += 1;
            let (ko, kl) = self.str32(&n.kind);
            self.rec.extend_from_slice(&id.to_le_bytes());
            self.rec.extend_from_slice(&parent.to_le_bytes());
            self.rec.extend_from_slice(&ko.to_le_bytes());
            self.rec.extend_from_slice(&kl.to_le_bytes());
            self.rec
                .extend_from_slice(&(n.attrs.len() as u32).to_le_bytes());
            for (k, v) in &n.attrs {
                let (o, l) = self.str32(k);
                self.rec.extend_from_slice(&o.to_le_bytes());
                self.rec.extend_from_slice(&l.to_le_bytes());
                // The tag occupies FOUR bytes, not one: the Java decoder reads the
                // tag then three padding bytes, so an f64 stays 8-byte aligned in the
                // record stream. Writing a bare byte here desynchronises every
                // attribute after the first — verified against Node.java rather than
                // assumed.
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
            for c in &n.children {
                self.walk(c, id);
            }
        }
    }
    let mut e = Enc {
        rec: Vec::new(),
        blob: Vec::new(),
        count: 0,
        next_id: 0,
    };
    e.walk(root, u32::MAX);
    let mut out = Vec::with_capacity(12 + e.rec.len() + e.blob.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.extend_from_slice(&e.count.to_le_bytes());
    out.extend_from_slice(&(e.blob.len() as u32).to_le_bytes());
    out.extend_from_slice(&e.rec);
    out.extend_from_slice(&e.blob);
    out
}

/// Text ROLES, resolved to this backend's own type scale.
///
/// The makepad lowering resolves the same seven roles to Roboto weights and pixel
/// sizes; here they resolve to Material 3 type tokens, so a native card gets the
/// PLATFORM's typography rather than a transplant of makepad's. That is the role
/// abstraction earning its keep — the plan names a role and each backend answers in its
/// own design language.
mod role {
    pub const HERO: &str = "displayMedium";
    pub const TITLE: &str = "headlineSmall";
    pub const BODY: &str = "titleMedium";
    pub const STAT: &str = "bodyMedium";
    pub const ROW: &str = "bodyLarge";
    pub const CAPTION: &str = "labelMedium";
    pub const VALUE: &str = "headlineSmall";
}

fn txt(role: &str, text: &str) -> Node {
    Node::new("text").s("text", text).s("variant", role)
}

/// A card wrapping its children in ONE column.
///
/// `card` builds a `MaterialCardView`, which is a **FrameLayout** — several direct
/// children STACK rather than flow. Discovered the hard way: seven forecast rows drew
/// on top of each other, which read as a data bug and was a container bug.
fn card(kids: Vec<Node>) -> Node {
    Node::new("card")
        .s("variant", "filled")
        .n("pad", 14.0)
        .kid(Node::new("col").n("spacing", 6.0).kids(kids))
}

/// Lower a plan to a node tree for the native-Android backend.
///
/// Returns a VISIBLE rejection rather than an error, for the same reason the makepad
/// path does: a blank screen cannot be told apart from a crash.
pub fn lower_plan_to_nodes(json: &str) -> Node {
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => return reject(&format!("plan is not valid JSON: {e}")),
    };
    let kind = v.get("plan").and_then(|k| k.as_str()).unwrap_or("");
    let locale = v.get("locale").and_then(|l| l.as_str()).unwrap_or("en");
    let loc = locale_tag(locale);
    let zh = loc == "zh";
    let Some(sections) = v.get("sections").and_then(|s| s.as_array()) else {
        return reject("plan has no sections");
    };
    match kind {
        "weather" => {
            let Some(place) = v
                .get("place")
                .and_then(|p| p.get("query"))
                .and_then(|q| q.as_str())
            else {
                return reject("weather plan has no place.query");
            };
            page(weather(sections, place, loc, zh))
        }
        "news" => page(news(sections, zh)),
        other => reject(&format!("unknown plan kind {other:?}")),
    }
}

fn page(kids: Vec<Node>) -> Node {
    Node::new("col").n("pad", 18.0).n("spacing", 12.0).kids(kids)
}

fn reject(msg: &str) -> Node {
    page(vec![
        txt(role::TITLE, "Plan rejected"),
        txt(role::STAT, msg),
    ])
}

/// `sys.*` call text, emitted verbatim into the node tree.
///
/// The node carries the CALL, not the answer — exactly as the DSL lowering does. The
/// rule holds on both backends: a card never contains a value the model supplied.
fn lat(place: &str) -> String {
    format!("sys.geocodenum({place:?}, \"lat\")")
}
fn lon(place: &str) -> String {
    format!("sys.geocodenum({place:?}, \"lon\")")
}

fn weather(sections: &[serde_json::Value], place: &str, loc: &str, zh: bool) -> Vec<Node> {
    let ll = format!("{}, {}", lat(place), lon(place));
    let mut out = Vec::new();
    for sec in sections {
        let block = sec.get("block").and_then(|b| b.as_str()).unwrap_or("");
        let args = sec.get("args");
        match block {
            "CurrentConditions" => out.push(
                Node::new("col")
                    .n("spacing", 2.0)
                    .kid(txt(role::TITLE, &format!("sys.geocode({place:?}, \"name\")")).s("bind", "1"))
                    .kid(
                        txt(
                            role::HERO,
                            &format!("sys.weather({ll}, \"current.temperature_2m\") + \"°\""),
                        )
                        .s("bind", "1"),
                    )
                    .kid(
                        Node::new("row")
                            .n("spacing", 8.0)
                            .kid(
                                Node::new("weathericon")
                                    .s("bind_cond", &format!("sys.weathercond({ll}, \"current.weather_code\")"))
                                    .n("w", 44.0)
                                    .n("h", 44.0),
                            )
                            .kid(
                                txt(
                                    role::BODY,
                                    &format!("sys.weatherword({ll}, \"current.weather_code\", {loc:?})"),
                                )
                                .s("bind", "1"),
                            ),
                    )
                    .kid(
                        txt(
                            role::STAT,
                            &format!(
                                "\"↑\" + sys.weather({ll}, \"daily.temperature_2m_max.0\") + \"°   ↓\" \
                                 + sys.weather({ll}, \"daily.temperature_2m_min.0\") + \"°   ≈\" \
                                 + sys.weather({ll}, \"current.apparent_temperature\") + \"°\""
                            ),
                        )
                        .s("bind", "1"),
                    ),
            ),
            "Forecast" => {
                let days = args
                    .and_then(|a| a.get("days"))
                    .and_then(|d| d.as_u64())
                    .unwrap_or(7)
                    .clamp(1, 7) as usize;
                let mut rows = Vec::new();
                for d in 0..days {
                    rows.push(
                        Node::new("row")
                            .n("spacing", 10.0)
                            .kid(txt(role::ROW, &format!("sys.dayname({ll}, {d}, {loc:?})")).s("bind", "1").n("w", 74.0))
                            .kid(
                                Node::new("weathericon")
                                    .s("bind_cond", &format!("sys.weathercond({ll}, \"daily.weather_code.{d}\")"))
                                    .n("w", 26.0)
                                    .n("h", 26.0),
                            )
                            .kid(
                                txt(role::ROW, &format!("sys.weather({ll}, \"daily.temperature_2m_min.{d}\") + \"°\""))
                                    .s("bind", "1")
                                    .n("w", 46.0),
                            )
                            .kid(
                                txt(role::ROW, &format!("sys.weather({ll}, \"daily.temperature_2m_max.{d}\") + \"°\""))
                                    .s("bind", "1")
                                    .n("w", 46.0),
                            ),
                    );
                }
                out.push(card(rows));
            }
            "AirQualityField" => out.push(card(vec![
                txt(role::CAPTION, if zh { "空气质量" } else { "AIR QUALITY" }),
                txt(role::VALUE, &format!("sys.airquality({ll}, \"current.us_aqi\")")).s("bind", "1"),
            ])),
            "SunMoon" => out.push(card(vec![
                txt(role::CAPTION, if zh { "日出 / 日落" } else { "SUNRISE / SUNSET" }),
                txt(
                    role::BODY,
                    &format!(
                        "sys.weather({ll}, \"daily.sunrise.0\") + \"   \" \
                         + sys.weather({ll}, \"daily.sunset.0\")"
                    ),
                )
                .s("bind", "1"),
            ])),
            "Details" => {
                let tiles: Vec<String> = args
                    .and_then(|a| a.get("tiles"))
                    .and_then(|t| t.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let mut cells = Vec::new();
                for k in &tiles {
                    let (cap, call) = match k.as_str() {
                        "uv" => (
                            if zh { "紫外线" } else { "UV INDEX" },
                            format!("sys.weather({ll}, \"daily.uv_index_max.0\")"),
                        ),
                        "humidity" => (
                            if zh { "湿度" } else { "HUMIDITY" },
                            format!("sys.weather({ll}, \"current.relative_humidity_2m\") + \"%\""),
                        ),
                        "wind" => (
                            if zh { "风速" } else { "WIND" },
                            format!("sys.weather({ll}, \"current.wind_speed_10m\") + \" km/h\""),
                        ),
                        "aqi" => (
                            if zh { "空气质量" } else { "AIR QUALITY" },
                            format!("sys.airquality({ll}, \"current.us_aqi\")"),
                        ),
                        _ => continue,
                    };
                    cells.push(
                        card(vec![txt(role::CAPTION, cap), txt(role::VALUE, &call).s("bind", "1")])
                            .n("w", 150.0),
                    );
                }
                for pair in cells.chunks(2) {
                    out.push(Node::new("row").n("spacing", 10.0).kids(pair.to_vec()));
                }
            }
            other => out.push(card(vec![txt(role::STAT, &format!("unknown block {other:?}"))])),
        }
    }
    out
}

fn news(sections: &[serde_json::Value], zh: bool) -> Vec<Node> {
    let mut out = Vec::new();
    for sec in sections {
        let block = sec.get("block").and_then(|b| b.as_str()).unwrap_or("");
        let args = sec.get("args");
        let arg = |k: &str, d: &str| {
            args.and_then(|a| a.get(k))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(d)
                .to_string()
        };
        match block {
            "Masthead" => out.push(
                Node::new("col")
                    .n("spacing", 2.0)
                    .kid(txt(role::CAPTION, &arg("label", if zh { "头条" } else { "TOP STORIES" })))
                    .kid(txt(role::HERO, &arg("title", if zh { "新闻" } else { "News" }))),
            ),
            "LeadStory" => out.push(card(vec![
                txt(role::CAPTION, if zh { "焦点" } else { "LEAD" }),
                txt(role::BODY, "sys.news(0, \"title\")").s("bind", "1"),
                txt(
                    role::CAPTION,
                    &format!(
                        "sys.news(0, \"points\") + {pts:?} + \" · \" + sys.news(0, \"author\")",
                        pts = if zh { " 分" } else { " pts" }
                    ),
                )
                .s("bind", "1"),
            ])),
            "StoryFeed" => {
                let n = args
                    .and_then(|a| a.get("count"))
                    .and_then(|c| c.as_u64())
                    .unwrap_or(7)
                    .clamp(1, 20) as usize;
                let mut rows = Vec::new();
                for r in 1..=n {
                    rows.push(
                        Node::new("row")
                            .n("spacing", 10.0)
                            .kid(txt(role::ROW, &r.to_string()).n("w", 26.0))
                            .kid(
                                Node::new("col")
                                    .n("spacing", 2.0)
                                    .kid(txt(role::ROW, &format!("sys.news({r}, \"title\")")).s("bind", "1"))
                                    .kid(
                                        txt(
                                            role::CAPTION,
                                            &format!(
                                                "sys.news({r}, \"points\") + {pts:?} + \" · \" \
                                                 + sys.news({r}, \"author\")",
                                                pts = if zh { " 分" } else { " pts" }
                                            ),
                                        )
                                        .s("bind", "1"),
                                    ),
                            ),
                    );
                }
                out.push(txt(role::CAPTION, &arg("label", if zh { "最新" } else { "LATEST" })));
                out.push(card(rows));
            }
            other => out.push(card(vec![txt(role::STAT, &format!("unknown block {other:?}"))])),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const KYOTO: &str = r#"{
        "plan": "weather", "locale": "en", "place": { "query": "Kyoto" },
        "sections": [
            { "block": "CurrentConditions" },
            { "block": "Forecast", "args": { "days": 7 } },
            { "block": "Details", "args": { "tiles": ["uv","humidity"] } }
        ]
    }"#;

    /// Decode the buffer back, so the encoder is checked against a reader rather than
    /// against itself.
    fn decode_kinds(buf: &[u8]) -> Vec<String> {
        assert_eq!(
            u32::from_le_bytes(buf[0..4].try_into().unwrap()),
            MAGIC,
            "magic must match Node.MAGIC on the Java side"
        );
        let count = u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize;
        let blob_len = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
        let blob_start = buf.len() - blob_len;
        let blob = &buf[blob_start..];
        let mut p = 12;
        let mut kinds = Vec::new();
        for _ in 0..count {
            p += 8; // id, parent
            let ko = u32::from_le_bytes(buf[p..p + 4].try_into().unwrap()) as usize;
            let kl = u32::from_le_bytes(buf[p + 4..p + 8].try_into().unwrap()) as usize;
            p += 8;
            kinds.push(String::from_utf8(blob[ko..ko + kl].to_vec()).unwrap());
            let na = u32::from_le_bytes(buf[p..p + 4].try_into().unwrap()) as usize;
            p += 4;
            for _ in 0..na {
                p += 8; // key
                let tag = buf[p];
                p += 4; // tag + 3 padding, matching Node.java
                assert!(tag == T_F64 || tag == T_STR, "unknown attr tag {tag}");
                p += 8; // f64, or (offset, len)
            }
        }
        kinds
    }

    #[test]
    fn the_buffer_round_trips() {
        let buf = encode(&lower_plan_to_nodes(KYOTO));
        let kinds = decode_kinds(&buf);
        assert_eq!(kinds[0], "col", "root is the page column");
        assert!(kinds.iter().any(|k| k == "weathericon"));
        assert!(kinds.iter().any(|k| k == "card"));
        // 1 current + 7 forecast rows.
        assert_eq!(kinds.iter().filter(|k| *k == "weathericon").count(), 8);
    }

    /// The node tree carries `sys.*` CALLS, never resolved values — the same rule the
    /// DSL lowering follows, so neither backend can contain a model-supplied fact.
    #[test]
    fn nodes_carry_calls_not_values() {
        let n = lower_plan_to_nodes(KYOTO);
        let mut found = 0;
        fn walk(n: &Node, found: &mut usize) {
            for (_, v) in &n.attrs {
                if let Val::S(s) = v {
                    if s.contains("sys.") {
                        *found += 1;
                    }
                    assert!(
                        !s.contains("35.0") && !s.contains("135.7"),
                        "a coordinate must never be baked in: {s}"
                    );
                }
            }
            for c in &n.children {
                walk(c, found);
            }
        }
        walk(&n, &mut found);
        assert!(found > 10, "expected many sys.* calls, found {found}");
    }

    #[test]
    fn a_bad_plan_renders_a_visible_rejection() {
        let n = lower_plan_to_nodes("{ not json");
        let buf = encode(&n);
        assert!(!buf.is_empty(), "never an empty buffer — a blank screen reads as a crash");
        let kinds = decode_kinds(&buf);
        assert!(kinds.iter().any(|k| k == "text"));
    }

    #[test]
    fn news_lowers_too() {
        let plan = r#"{"plan":"news","locale":"en","sections":[
            {"block":"Masthead","args":{"title":"Top Stories"}},
            {"block":"LeadStory"},
            {"block":"StoryFeed","args":{"count":5}}]}"#;
        let kinds = decode_kinds(&encode(&lower_plan_to_nodes(plan)));
        assert!(kinds.iter().filter(|k| *k == "row").count() >= 5);
    }
}

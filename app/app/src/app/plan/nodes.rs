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


/// Plain-data source for a plan, or `None` if this plan kind has no lowering here.
///
/// Used by the card saver to publish the registry-free form beside the makepad one, so a
/// backend without makepad's widget registry renders the model's REAL output rather than
/// a hand-written stand-in.
pub fn try_plain(plan_json: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(plan_json)
        .map_err(|e| format!("plan is not JSON ({e}); first 80 bytes: {:?}", &plan_json.chars().take(80).collect::<String>()))?;
    let kind = v
        .get("plan")
        .and_then(|k| k.as_str())
        .ok_or_else(|| "plan has no \"plan\" field".to_string())?;
    match kind {
        "weather" | "news" | "stock" => Ok(to_plain_splash(&lower_plan_to_nodes(plan_json))),
        other => Err(format!("no plain-data lowering for kind {other:?}")),
    }
}

/// Serialise a node tree as PLAIN-DATA SPLASH SOURCE.
///
/// This is the form `ymote/Splash` evaluates and `ymote/Splash-Android` renders:
///
/// ```text
/// {t: "col", pad: 18, c: [
///     {t: "text", text: sys.geocode("Kyoto", "name"), variant: "headlineSmall"},
/// ]}
/// ```
///
/// Note what it is NOT: makepad dialect. A makepad card says `SolidView{…}` /
/// `TextHero{…}`, which resolves through makepad's WIDGET REGISTRY — so on a backend
/// without `makepad-widgets` it evaluates to an object with no `t:` tag and no tree.
/// That is not a bug to fix in the renderer; the two dialects are genuinely different
/// vocabularies over one language, and the plan is what lets a card exist in both.
///
/// String attributes holding a `sys.*` call are emitted as EXPRESSIONS, not quoted
/// text, so the Android VM resolves them at render time exactly as makepad does. The
/// rule survives the port: a card carries calls, never values.
pub fn to_plain_splash(root: &Node) -> String {
    let mut out = String::from("// name: plain-card\n// LOWERED from a semantic plan for the plain-data backend.\n");
    write_node(root, 0, &mut out);
    out.push('\n');
    out
}

/// A value that should be emitted as a Splash expression rather than a quoted string.
///
/// `sys.foo(...)`, and concatenations built from one. Anything else is literal text and
/// must be quoted, or a headline containing a bracket would become a syntax error.
fn is_expr(s: &str) -> bool {
    s.contains("sys.") && (s.starts_with("sys.") || s.starts_with('"'))
}

fn write_node(n: &Node, depth: usize, out: &mut String) {
    let pad = "    ".repeat(depth);
    out.push_str(&format!("{pad}{{t: {:?}", n.kind));
    for (k, v) in &n.attrs {
        // A `*_expr` attribute carries a sys.* CALL and is emitted unquoted under its
        // base name, so the VM evaluates it and the builder reads a number. Quoting it
        // would render the call as literal text — the failure this naming exists to make
        // impossible to introduce by accident.
        let (name, force_expr) = match k.strip_suffix("_expr") {
            Some(base) => (base, true),
            None => (k.as_str(), false),
        };
        match v {
            Val::F(f) => out.push_str(&format!(", {name}: {f}")),
            Val::S(s) if force_expr || is_expr(s) => out.push_str(&format!(", {name}: {s}")),
            Val::S(s) => out.push_str(&format!(", {name}: {s:?}")),
        }
    }
    if n.children.is_empty() {
        out.push('}');
        return;
    }
    out.push_str(", c: [\n");
    for (i, c) in n.children.iter().enumerate() {
        write_node(c, depth + 1, out);
        if i + 1 < n.children.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str(&format!("{pad}]}}"));
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
    // Translucent on purpose: over a backdrop an opaque panel reads as a box pasted on
    // top, while a wash lets the photo carry through and the card feel part of it.
    Node::new("card")
        .s("variant", "filled")
        .n("bg", 0x8C_10_14_1Cu32 as f64)
        .n("radius", 18.0)
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
            let photo = v.get("photo").and_then(|p| p.as_str()).unwrap_or(place);
            photo_page(photo, weather(sections, place, loc, zh))
        }
        "news" => page(news(sections, zh)),
        "stock" => page(stock(sections, zh)),
        other => reject(&format!("unknown plan kind {other:?}")),
    }
}

fn page(kids: Vec<Node>) -> Node {
    Node::new("col").n("pad", 18.0).n("spacing", 12.0).kids(kids)
}

/// A page over a full-bleed backdrop.
///
/// The plan has carried `photo` from the start — it is the model's own words for what
/// the place looks like, which is exactly the kind of thing no tool can answer. The
/// native backend simply had no `sys.photo` to resolve it, so the card fell back to a
/// flat dark panel and looked drab beside the makepad one. That was a missing data
/// helper, not a missing renderer.
///
/// A scrim sits between photo and content: the backdrop is an arbitrary generated image,
/// so text over it is only legible if something guarantees contrast.
fn photo_page(query: &str, kids: Vec<Node>) -> Node {
    Node::new("stack").kids(vec![
        Node::new("image").s("src_expr", &format!("sys.photo({query:?})")),
        Node::new("box").n("bg", 0x99_0A_0E_14u32 as f64),
        page(kids),
    ])
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
                    // Centred, like the makepad hero. A left-aligned hero reads as a
                    // draft — the same note that is in the weather spec.
                    .n("align", 1.0)
                    .n("spacing", 2.0)
                    .kid(txt(role::TITLE, &format!("sys.geocode({place:?}, \"name\")")))
                    .kid(
                        txt(
                            role::HERO,
                            &format!("sys.weather({ll}, \"current.temperature_2m\") + \"°\""),
                        )
                        ,
                    )
                    .kid(
                        Node::new("row")
                            .n("align", 1.0)
                            .n("spacing", 8.0)
                            .kid(
                                Node::new("weathericon")
                                    .s("cond_expr", &format!("sys.weathercond({ll}, \"current.weather_code\")"))
                                    .n("w", 44.0)
                                    .n("h", 44.0),
                            )
                            .kid(
                                txt(
                                    role::BODY,
                                    &format!("sys.weatherword({ll}, \"current.weather_code\", {loc:?})"),
                                )
                                ,
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
                        ,
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
                            .kid(txt(role::ROW, &format!("sys.dayname({ll}, {d}, {loc:?})")).n("w", 74.0))
                            .kid(
                                Node::new("weathericon")
                                    .s("cond_expr", &format!("sys.weathercond({ll}, \"daily.weather_code.{d}\")"))
                                    .n("w", 26.0)
                                    .n("h", 26.0),
                            )
                            .kid(
                                txt(role::ROW, &format!("sys.weather({ll}, \"daily.temperature_2m_min.{d}\") + \"°\""))
                                    .n("w", 44.0),
                            )
                            // The range bar. Its endpoints and the week's extent are all
                            // live calls: a plan cannot state the week's range, because
                            // the values are a fetch it never sees.
                            .kid(
                                Node::new("tempbar")
                                    .s("lo_expr", &format!("sys.weathernum({ll}, \"daily.temperature_2m_min.{d}\")"))
                                    .s("hi_expr", &format!("sys.weathernum({ll}, \"daily.temperature_2m_max.{d}\")"))
                                    .s("wmin_expr", &format!("sys.weekmin({ll})"))
                                    .s("wmax_expr", &format!("sys.weekmax({ll})"))
                                    .n("h", 6.0)
                                    .n("grow", 1.0),
                            )
                            .kid(
                                txt(role::ROW, &format!("sys.weather({ll}, \"daily.temperature_2m_max.{d}\") + \"°\""))
                                    .n("w", 44.0),
                            ),
                    );
                }
                out.push(card(rows));
            }
            "AirQualityField" => out.push(card(vec![
                txt(role::CAPTION, if zh { "空气质量" } else { "AIR QUALITY" }),
                txt(role::VALUE, &format!("sys.airquality({ll}, \"current.us_aqi\")")),
            ])),
            "SunMoon" => out.push(card(vec![
                txt(role::CAPTION, if zh { "日出 / 日落" } else { "SUNRISE / SUNSET" }),
                Node::new("sunarc")
                    .s("progress_expr", &format!("sys.daylight({ll})"))
                    .n("h", 76.0),
                Node::new("row").n("spacing", 12.0).kids(vec![
                    txt(role::ROW, &format!("sys.weather({ll}, \"daily.sunrise.0\")")),
                    txt(role::ROW, &format!("sys.weather({ll}, \"daily.sunset.0\")")).n("grow", 1.0),
                ]),
                // 月相. Drawn from the live phase, with its name and lit percentage —
                // the makepad card's SunMoon block has all three and this had none.
                Node::new("row").n("align", 1.0).n("spacing", 14.0).kids(vec![
                    Node::new("moonphase")
                        .s("phase_expr", "sys.moonnum(\"phase\")")
                        .n("w", 56.0)
                        .n("h", 56.0),
                    Node::new("col").n("spacing", 2.0).n("grow", 1.0).kids(vec![
                        txt(
                            role::BODY,
                            &format!("sys.moonphase({:?})", if zh { "name_zh" } else { "name" }),
                        ),
                        txt(
                            role::CAPTION,
                            &format!(
                                "sys.moonphase(\"illumination\") + {:?}",
                                if zh { "% 照亮" } else { "% illuminated" }
                            ),
                        ),
                    ]),
                ]),
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
                        card(vec![txt(role::CAPTION, cap), txt(role::VALUE, &call)])
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
                txt(role::BODY, "sys.news(0, \"title\")"),
                txt(
                    role::CAPTION,
                    &format!(
                        "sys.news(0, \"points\") + {pts:?} + \" · \" + sys.news(0, \"author\")",
                        pts = if zh { " 分" } else { " pts" }
                    ),
                )
                ,
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
                                    .kid(txt(role::ROW, &format!("sys.news({r}, \"title\")")))
                                    .kid(
                                        txt(
                                            role::CAPTION,
                                            &format!(
                                                "sys.news({r}, \"points\") + {pts:?} + \" · \" \
                                                 + sys.news({r}, \"author\")",
                                                pts = if zh { " 分" } else { " pts" }
                                            ),
                                        )
                                        ,
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


/// Market blocks for the plain-data backend.
///
/// The ticker is the one fact a plan carries, because resolving "apple" to AAPL is world
/// knowledge. Everything the symbol MEANS — price, change, direction, company name — is a
/// live call, and the direction is the subtle one: a plan asserting "up" would paint a red
/// day green for as long as the card exists, so the arrow and the colour both come from
/// `sys.stockrange(..., "up")` at render time.
fn stock(sections: &[serde_json::Value], zh: bool) -> Vec<Node> {
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
        let ticker = arg("ticker", "");
        let range = arg("range", "1d");
        match block {
            "MoversList" => {
                let n = args
                    .and_then(|a| a.get("count"))
                    .and_then(|c| c.as_u64())
                    .unwrap_or(10)
                    .clamp(1, 10) as usize;
                out.push(
                    Node::new("col").n("spacing", 2.0).kids(vec![
                        txt(role::CAPTION, &arg("label", if zh { "今日涨幅榜" } else { "TODAY · TOP GAINERS" })),
                        txt(role::HERO, &arg("title", if zh { "涨幅榜" } else { "Movers" })),
                    ]),
                );
                let mut rows = Vec::new();
                for r in 0..n {
                    rows.push(
                        Node::new("row").n("spacing", 10.0).kids(vec![
                            txt(role::ROW, &format!("{}", r + 1)).n("w", 26.0),
                            Node::new("col").n("spacing", 2.0).n("grow", 1.0).kids(vec![
                                txt(role::ROW, &format!("sys.movers({r}, \"symbol\")")),
                                txt(role::CAPTION, &format!("sys.movers({r}, \"name\")")),
                            ]),
                            Node::new("col").n("spacing", 2.0).kids(vec![
                                txt(role::ROW, &format!("\"$\" + sys.movers({r}, \"price\")")),
                                txt(role::CAPTION, &format!("sys.movers({r}, \"changepct\")")),
                            ]),
                        ]),
                    );
                    if r + 1 < n {
                        rows.push(Node::new("divider"));
                    }
                }
                out.push(card(rows));
            }
            "QuoteHeader" => out.push(Node::new("col").n("spacing", 2.0).kids(vec![
                txt(role::TITLE, &format!("sys.stock({ticker:?}, \"symbol\")")),
                txt(role::CAPTION, &format!("sys.stock({ticker:?}, \"name\")")),
                txt(role::HERO, &format!("\"$\" + sys.stock({ticker:?}, \"price\")")),
                txt(
                    role::STAT,
                    &format!(
                        "sys.stockrange({ticker:?}, {range:?}, \"change\") + \"  (\" \
                         + sys.stockrange({ticker:?}, {range:?}, \"changepct\") + \")\""
                    ),
                ),
            ])),
            "StatGrid" => {
                let stats: Vec<String> = args
                    .and_then(|a| a.get("stats"))
                    .and_then(|t| t.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                let mut cells = Vec::new();
                for k in &stats {
                    let cap = match k.as_str() {
                        "price" => if zh { "现价" } else { "PRICE" },
                        "prev" => if zh { "昨收" } else { "PREV CLOSE" },
                        "high" => if zh { "最高" } else { "HIGH" },
                        "low" => if zh { "最低" } else { "LOW" },
                        "open" => if zh { "开盘" } else { "OPEN" },
                        "currency" => if zh { "货币" } else { "CURRENCY" },
                        _ => continue,
                    };
                    cells.push(
                        card(vec![
                            txt(role::CAPTION, cap),
                            txt(role::VALUE, &format!("sys.stock({ticker:?}, {k:?})")),
                        ])
                        .n("grow", 1.0),
                    );
                }
                for pair in cells.chunks(2) {
                    out.push(Node::new("row").n("spacing", 10.0).kids(pair.to_vec()));
                }
            }
            // PriceChart needs a plotting surface this backend does not have. Named on
            // screen rather than dropped: a silently missing chart looks like a card that
            // simply has none.
            "PriceChart" => out.push(card(vec![
                txt(role::CAPTION, if zh { "价格走势" } else { "PRICE CHART" }),
                txt(role::STAT, if zh { "此后端暂无绘图组件" } else { "no plotting surface on this backend" }),
            ])),
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
        "photo": "kyoto temple autumn mist cinematic",
        "sections": [
            { "block": "CurrentConditions" },
            { "block": "Forecast", "args": { "days": 7 } },
            { "block": "AirQualityField" },
            { "block": "SunMoon" },
            { "block": "Details", "args": { "tiles": ["uv","humidity","wind"] } }
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
        // A weather page roots at a `stack`: backdrop, scrim, content. The assertion
        // used to demand `col` and correctly failed when the backdrop landed.
        assert_eq!(kinds[0], "stack", "weather roots at the backdrop stack");
        assert!(kinds.iter().any(|k| k == "image"), "the backdrop must be there");
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


    /// The plain-data form must be real Splash source: `t:` tags, and `sys.*` emitted as
    /// CALLS rather than quoted text — otherwise the Android VM would render the literal
    /// string "sys.weather(...)" instead of the temperature.
    #[test]
    fn plain_source_carries_calls_as_expressions() {
        let src = to_plain_splash(&lower_plan_to_nodes(KYOTO));
        assert!(src.contains("{t: \"col\""), "plain-data root: {src:.120}");
        assert!(
            src.contains("text: sys.geocode(") || src.contains("text: sys.weather("),
            "sys.* must be an expression, not a quoted string"
        );
        assert!(
            !src.contains("text: \"sys."),
            "a quoted sys.* call would render as literal text"
        );
        // No makepad dialect may leak in — that is what fails on a registry-free backend.
        for w in ["SolidView", "TextHero", "RoundedView", "draw_bg", "draw_text"] {
            assert!(!src.contains(w), "makepad dialect leaked: {w}");
        }
    }


    /// Writes the plain-data card to /tmp so it can be pushed to the device. Ignored by
    /// default — it is a build artifact, not an assertion.
    #[test]
    #[ignore]
    fn dump_plain_card() {
        let src = to_plain_splash(&lower_plan_to_nodes(KYOTO));
        std::fs::write("/tmp/plain-weather.splash", &src).unwrap();
        let news = r#"{"plan":"news","locale":"en","sections":[
            {"block":"Masthead","args":{"title":"Top Stories","label":"HACKER NEWS"}},
            {"block":"LeadStory"},
            {"block":"StoryFeed","args":{"count":6}}]}"#;
        std::fs::write(
            "/tmp/plain-news.splash",
            to_plain_splash(&lower_plan_to_nodes(news)),
        )
        .unwrap();
        let stock = r#"{"plan":"stock","locale":"en","sections":[
            {"block":"MoversList","args":{"count":8}}]}"#;
        std::fs::write(
            "/tmp/plain-stock.splash",
            to_plain_splash(&lower_plan_to_nodes(stock)),
        )
        .unwrap();
    }

    /// Every domain on the PLAN path must also lower for the native backend. Without
    /// this, a domain can join PLAN_DOMAINS and silently render as "unknown plan kind"
    /// here — which is how stock was missing while weather and news looked finished.
    #[test]
    fn every_plan_domain_reaches_the_native_backend() {
        for d in super::super::PLAN_DOMAINS {
            let minimal = match *d {
                "weather" => r#"{"plan":"weather","locale":"en","place":{"query":"Kyoto"},
                    "sections":[{"block":"CurrentConditions"}]}"#,
                "news" => r#"{"plan":"news","locale":"en",
                    "sections":[{"block":"Masthead","args":{"title":"News"}}]}"#,
                "stock" => r#"{"plan":"stock","locale":"en",
                    "sections":[{"block":"MoversList","args":{"count":3}}]}"#,
                other => panic!("PLAN_DOMAINS has {other:?} with no native test plan"),
            };
            let src = to_plain_splash(&lower_plan_to_nodes(minimal));
            assert!(
                !src.contains("unknown plan kind"),
                "{d} does not lower for the native backend"
            );
            assert!(
                src.contains("{t: \"col\"") || src.contains("{t: \"stack\""),
                "{d} must produce a plain-data tree"
            );
        }
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

//! The durable store — profile §5.12.
//!
//! What a user chose, kept across restarts. Two kinds live here and they are the
//! only two the profile admits:
//!
//! - **collections** — ordered sets of entity REFERENCES (`["NVDA", "AAPL"]`)
//! - **prefs** — single values (`units = "c"`)
//!
//! **References, never facts.** A watchlist holds tickers; the prices rendered
//! beside them are fetched every time it is read. §4's no-facts rule does not
//! stop applying because the data went to disk — a stored price is wrong a
//! second after it is written, and a stale number that still looks live is the
//! exact failure that rule exists to prevent. The card's own `$181` open was
//! that bug with a fixture standing in for a store.
//!
//! **Why the card cannot hold this.** Cards are regenerated per request, so a
//! list-shaped state cell would be empty again the next time the user asked —
//! and would look like it worked until they came back.

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::RwLock;

/// Bumped when the on-disk shape changes in a way an older reader would
/// misread. A file stamped with anything else is discarded rather than guessed
/// at — §5.8's rule, applied to the store: feeding version-A data to version-B
/// code produces a screen that renders confidently and wrongly.
const VERSION: u64 = 1;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UserStore {
    #[serde(default)]
    pub version: u64,
    /// Ordered. The order IS the user's order — it is what a reorder edits, and
    /// why this is an array and not a set.
    #[serde(default)]
    pub collections: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub prefs: std::collections::BTreeMap<String, String>,
}

static STORE: RwLock<Option<UserStore>> = RwLock::new(None);

/// `files/.config/octos-app/user.json`, beside the server config that already
/// lives there.
fn path() -> Option<PathBuf> {
    let dir = super::login::config_dir()?;
    Some(dir.join("user.json"))
}

/// Read the store, loading it once.
///
/// A file that cannot be parsed, or that carries a version this build does not
/// know, is treated as absent. That loses the user's list, which is bad — and
/// better than the alternative, which is rendering someone else's data as
/// theirs.
pub fn get() -> UserStore {
    if let Ok(slot) = STORE.read() {
        if let Some(s) = slot.as_ref() {
            return s.clone();
        }
    }
    let loaded = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<UserStore>(&raw).ok())
        .filter(|s| s.version == VERSION)
        .unwrap_or_else(|| UserStore {
            version: VERSION,
            ..Default::default()
        });
    if let Ok(mut slot) = STORE.write() {
        *slot = Some(loaded.clone());
    }
    loaded
}

/// Persist, via a temp file and a rename.
///
/// Writing in place would leave a truncated file if the process died mid-write,
/// and this process is force-stopped constantly — a half-written JSON file is a
/// user who lost their list to a development convenience.
fn save(store: &UserStore) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(json) = serde_json::to_string_pretty(store) else {
        return;
    };
    let tmp = p.with_extension("json.tmp");
    let wrote = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(json.as_bytes())?;
        f.sync_all()
    });
    if wrote.is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Apply one write from `DispatchOutcome::writes`.
///
/// Returns whether anything changed, so a caller can skip the refetch and
/// redraw when a tap was a no-op — adding a ticker that is already there should
/// not look different from doing nothing, because it is nothing.
pub fn apply(collection: &str, op: &str, value: &str) -> bool {
    let mut store = get();
    let changed = match op {
        "append" => {
            let list = store.collections.entry(collection.to_owned()).or_default();
            // Idempotent. A watchlist is a set with an order, so adding twice is
            // not adding twice — and a duplicate row would key-collide in the
            // card's `for … key w.ticker`, which is a rendering bug §5.1 warns
            // about rather than a data one.
            if list.iter().any(|v| v == value) {
                false
            } else {
                list.push(value.to_owned());
                true
            }
        }
        "remove" => store
            .collections
            .get_mut(collection)
            .is_some_and(|list| {
                let before = list.len();
                list.retain(|v| v != value);
                list.len() != before
            }),
        "set" => {
            store.prefs.insert(collection.to_owned(), value.to_owned()) != Some(value.to_owned())
        }
        "clear" => store.prefs.remove(collection).is_some(),
        _ => false,
    };
    if changed {
        if let Ok(mut slot) = STORE.write() {
            *slot = Some(store.clone());
        }
        save(&store);
    }
    changed
}

/// Every collection, for handing to the VM in one go.
pub fn all_collections() -> std::collections::BTreeMap<String, Vec<String>> {
    get().collections
}

/// The references in a collection, in the user's order.
pub fn collection(name: &str) -> Vec<String> {
    get().collections.get(name).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The store holds REFERENCES and keeps their order.
    ///
    /// Order is not incidental: it is what the user arranged, and it is why a
    /// collection is an array. A set would make the list re-sort itself between
    /// launches for no reason the user could see.
    #[test]
    fn a_collection_is_an_ordered_set_of_references() {
        let mut s = UserStore {
            version: VERSION,
            ..Default::default()
        };
        let list = s.collections.entry("watchlist".into()).or_default();
        for t in ["NVDA", "AAPL", "TSLA"] {
            list.push(t.to_string());
        }
        let json = serde_json::to_string(&s).expect("serializes");
        let back: UserStore = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(
            back.collections["watchlist"],
            vec!["NVDA", "AAPL", "TSLA"],
            "the user's order must survive the round trip"
        );
        // And nothing else came along for the ride. A price here would be stale
        // the moment it was written.
        assert!(
            !json.contains("last") && !json.contains('.'),
            "a collection stores references, never facts: {json}"
        );
    }

    /// A file from a future or unknown version is discarded, not guessed at.
    #[test]
    fn a_foreign_version_is_not_read() {
        let raw = r#"{"version": 99, "collections": {"watchlist": ["NVDA"]}}"#;
        let parsed: Option<UserStore> = serde_json::from_str::<UserStore>(raw)
            .ok()
            .filter(|s| s.version == VERSION);
        assert!(
            parsed.is_none(),
            "a version this build does not know must be treated as absent"
        );
    }
}

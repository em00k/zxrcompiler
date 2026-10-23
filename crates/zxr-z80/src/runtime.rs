//! The runtime library's labels (`backend/runtime/*.py`): one constant per
//! routine the generated code calls, and the library file each lives in.
//! The table itself is data: `data/backend/runtime_labels.tsv`.

include!(concat!(env!("OUT_DIR"), "/runtime_labels.rs"));

/// The library file a call to `label` pulls in, if it needs one
/// (`LABEL_REQUIRED_MODULES`).
pub fn required_module(label: &str) -> Option<&'static str> {
    LABEL_REQUIRED_MODULES.binary_search_by(|(l, _)| (*l).cmp(label)).ok().map(|i| LABEL_REQUIRED_MODULES[i].1)
}

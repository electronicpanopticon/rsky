use super::{Leaf, NodeData, NodeEntry, TreeEntry, MST};
use crate::common::ipld;
use crate::storage::SqlRepoReader;
use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use libipld::Cid;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::str;

fn is_valid_chars(input: String) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"^[a-zA-Z0-9_\-:.]*$").unwrap();
    }
    RE.is_match(&input)
}

// * Restricted to a subset of ASCII characters — the allowed characters are
// alphanumeric (A-Za-z0-9), period, dash, underscore, colon, or tilde (.-_:~)
// * Must have at least 1 and at most 512 characters
// * The specific record key values . and .. are not allowed
pub fn is_valid_repo_mst_path(key: &String) -> Result<bool> {
    let split: Vec<&str> = key.split("/").collect();
    return if key.len() <= 256
        && split.len() == 2
        && split[0].len() > 0
        && split[1].len() > 0
        && is_valid_chars(split[0].to_owned())
        && is_valid_chars(split[1].to_owned())
    {
        Ok(true)
    } else {
        Ok(false)
    };
}

pub fn ensure_valid_mst_key(key: &String) -> Result<()> {
    let result = is_valid_repo_mst_path(key)?;
    match result {
        true => Ok(()),
        _ => Err(anyhow!("Invalid MST Key: {}", key)),
    }
}

pub fn cid_for_entries(entries: Vec<NodeEntry>) -> Result<Cid> {
    let data = serialize_node_data(entries)?;
    ipld::cid_for_cbor(&data)
}

pub fn count_prefix_len(a: String, b: String) -> Result<usize> {
    let mut x = 0;
    for i in 0..a.len() {
        if a.chars().nth(i).unwrap() != b.chars().nth(i).unwrap() {
            break;
        }
        x += 1;
    }
    Ok(x)
}

pub fn serialize_node_data(entries: Vec<NodeEntry>) -> Result<NodeData> {
    let mut data = NodeData {
        l: None,
        e: Vec::new(),
    };
    let mut i = 0;
    if let Some(NodeEntry::MST(e)) = &entries.get(0) {
        i += 1;
        data.l = Some(e.pointer);
    }
    let mut last_key = "";
    while i < entries.len() {
        let leaf = &entries[i];
        let next = &entries[i + 1];
        if !leaf.is_leaf() {
            return Err(anyhow!("Not a valid node: two subtrees next to each other"));
        };
        i += 1;
        let mut subtree: Option<Cid> = None;
        match next {
            NodeEntry::MST(tree) => {
                subtree = Some(tree.pointer);
                i += 1;
            }
            _ => (),
        };
        if let NodeEntry::Leaf(l) = leaf {
            ensure_valid_mst_key(&l.key)?;
            let prefix_len = count_prefix_len(last_key.to_owned(), l.key.to_owned())?;
            data.e.push(TreeEntry {
                p: u8::try_from(prefix_len)?,
                k: l.key[0..prefix_len].to_owned().into_bytes(),
                v: l.value,
                t: subtree,
            });
            last_key = &l.key;
        }
    }
    Ok(data)
}

pub fn deserialize_node_data(
    storage: &SqlRepoReader,
    data: NodeData,
    layer: Option<u32>,
) -> Result<Vec<NodeEntry>> {
    let mut entries: Vec<NodeEntry> = Vec::new();
    if let Some(l) = data.l {
        let new_layer: Option<u32>;
        if let Some(layer) = layer {
            new_layer = Some(layer - 1);
        } else {
            new_layer = None;
        }
        let mst = MST::load(storage.clone(), l, new_layer)?;
        let mst = NodeEntry::MST(mst);
        entries.push(mst)
    }
    let mut last_key: String = "".to_owned();
    for entry in data.e {
        let key_str = str::from_utf8(entry.k.as_ref())?;
        let p = usize::try_from(entry.p)?;
        let key = format!("{}{}", &last_key[0..p], key_str);
        ensure_valid_mst_key(&key)?;
        entries.push(NodeEntry::Leaf(Leaf {
            key: key.clone(),
            value: entry.v,
        }));
        last_key = key;
        if let Some(t) = entry.t {
            let new_layer: Option<u32>;
            if let Some(layer) = layer {
                new_layer = Some(layer - 1);
            } else {
                new_layer = None;
            }
            let mst = MST::load(storage.clone(), t, new_layer)?;
            let mst = NodeEntry::MST(mst);
            entries.push(mst)
        }
    }
    Ok(entries)
}

pub fn layer_for_entries(entries: Vec<NodeEntry>) -> Result<Option<u32>> {
    let first_leaf = entries.into_iter().find(|entry| entry.is_leaf());
    if let Some(f) = first_leaf {
        match f {
            NodeEntry::MST(_) => Ok(None),
            NodeEntry::Leaf(l) => Ok(Some(leading_zeros_on_hash(&l.key.to_owned().into_bytes())?)),
        }
    } else {
        return Ok(None);
    }
}

pub fn leading_zeros_on_hash(key: &Vec<u8>) -> Result<u32> {
    let digest = Sha256::digest(&*key);
    let hash: &[u8] = digest.as_ref();
    let mut leading_zeros = 0;
    for byte in hash {
        if *byte < 64 {
            leading_zeros += 1
        };
        if *byte < 16 {
            leading_zeros += 1
        };
        if *byte < 4 {
            leading_zeros += 1
        };
        if *byte == 0 {
            leading_zeros += 1;
        } else {
            break;
        }
    }
    Ok(leading_zeros)
}
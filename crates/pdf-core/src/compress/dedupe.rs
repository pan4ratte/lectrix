//! Merging duplicate objects before Compress writes its file (ADR 0027). MuPDF's own
//! (garbage collection at levels 3 and 4) compares every object with every other, again
//! and again until nothing changes: two and a half minutes for a book of 2,600 pages.
//! Here each candidate is hashed (its dictionary written out exactly, with a stream's raw
//! data), only objects with the same hash are compared, and references to a duplicate are
//! pointed at the first of its kind; garbage collection then leaves the duplicates out.
//! Merging may make objects that refer to merged ones equal in turn (fonts whose
//! programs were duplicates), so it goes round a few times.
//!
//! Only objects that may be shared are merged: streams (images, font programs, colour
//! profiles, page contents, appearances), arrays, and dictionaries of the types below.
//! Pages, page trees, annotations, fields and outline items stay one object each, and so
//! do the arrays that list them or that anything adds to (`/Annots`, `/Kids`, `/Fields`,
//! the keys in `CONTAINERS`): two pages sharing one empty `/Annots` would both show an
//! annotation added to either. An appearance stream shared by several annotations is
//! never changed in place: MuPDF's synthesis writes a new one, and Lectrix copies one from
//! the file before changing it (`annot::write::grow_with_appearance`).

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};

/// Dictionary types that may be shared.
const SHARED_TYPES: &[&[u8]] = &[
    b"Font",
    b"FontDescriptor",
    b"ExtGState",
    b"Encoding",
    b"Pattern",
    b"Shading",
    b"Group",
    b"Halftone",
    b"Mask",
];

/// Keys whose arrays are lists of their own: of a page's annotations, a node's kids, a
/// form's fields, a tree's entries, optional content's groups and their states, a page's
/// content streams, an article's beads. Such an array (indirect) is never merged.
const CONTAINERS: &[&[u8]] = &[
    b"Annots",
    b"Kids",
    b"Fields",
    b"CO",
    b"K",
    b"Nums",
    b"Names",
    b"Contents",
    b"OCGs",
    b"Order",
    b"ON",
    b"OFF",
    b"Locked",
    b"RBGroups",
    b"AS",
    b"Threads",
    b"B",
];

/// The most rounds: streams, then what refers to them, then what refers to that.
const ROUNDS: usize = 4;

/// Merges `doc`'s duplicate objects; says how many objects were left unreferenced.
pub(super) fn merge_duplicates(doc: &mut PdfDocument) -> Result<usize> {
    // The duplicates already left for garbage collection, and the lists never merged.
    let mut skip = containers(doc)?;
    let mut merged = 0;
    for _ in 0..ROUNDS {
        let remap = duplicates(doc, &skip)?;
        if remap.is_empty() {
            break;
        }
        merged += remap.len();
        repoint(doc, &remap)?;
        skip.extend(remap.keys());
    }
    Ok(merged)
}

/// The arrays (indirect) held under a key of `CONTAINERS`, anywhere in `doc`.
fn containers(doc: &PdfDocument) -> Result<HashSet<i32>> {
    let mut found = HashSet::new();
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    for num in 1..len {
        if let Some(obj) = doc.new_indirect(num, 0)?.resolve()? {
            containers_in(&obj, &mut found, 0)?;
        }
    }
    containers_in(&doc.trailer()?, &mut found, 0)?;
    Ok(found)
}

/// Adds the containers held directly in `obj` (not in the objects it refers to).
fn containers_in(obj: &PdfObject, found: &mut HashSet<i32>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH || obj.is_indirect()? {
        return Ok(());
    }
    if obj.is_dict()? {
        for i in 0..obj.dict_len()? {
            let (Some(key), Some(value)) =
                (obj.get_dict_key(i as i32)?, obj.get_dict_val(i as i32)?)
            else {
                continue;
            };
            if value.is_indirect()? {
                if CONTAINERS.contains(&key.as_name()?.as_slice()) {
                    found.insert(value.as_indirect()?);
                }
            } else {
                containers_in(&value, found, depth + 1)?;
            }
        }
    } else if obj.is_array()? {
        for i in 0..obj.len()? {
            if let Some(item) = obj.get_array(i as i32)? {
                containers_in(&item, found, depth + 1)?;
            }
        }
    }
    Ok(())
}

/// Object number of each duplicate, with the number of the object it duplicates.
fn duplicates(doc: &PdfDocument, keep: &HashSet<i32>) -> Result<HashMap<i32, i32>> {
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    let mut first: HashMap<(u64, u64), Vec<i32>> = HashMap::new();
    let mut remap = HashMap::new();
    for num in 1..len {
        if keep.contains(&num) {
            continue;
        }
        let obj = doc.new_indirect(num, 0)?;
        // An object too deep to write out is not merged.
        let Ok(Some(key)) = candidate(&obj) else {
            continue;
        };
        let hash = (hash_of(&key, 0), hash_of(&key, 1));
        let earlier = first.entry(hash).or_default();
        // Equal hashes are checked byte for byte.
        let mut same = None;
        for &other in earlier.iter() {
            if candidate(&doc.new_indirect(other, 0)?)
                .ok()
                .flatten()
                .as_ref()
                == Some(&key)
            {
                same = Some(other);
                break;
            }
        }
        match same {
            Some(other) => {
                remap.insert(num, other);
            }
            None => earlier.push(num),
        }
    }
    Ok(remap)
}

fn hash_of(bytes: &[u8], salt: u8) -> u64 {
    let mut h = DefaultHasher::new();
    salt.hash(&mut h);
    bytes.hash(&mut h);
    h.finish()
}

/// What makes `obj` (indirect) equal to another: its contents written out, or `None` if it
/// is not to be merged.
fn candidate(obj: &PdfObject) -> Result<Option<Vec<u8>>> {
    let Some(resolved) = obj.resolve()? else {
        return Ok(None);
    };
    let mut out = Vec::new();
    if obj.is_stream()? {
        if matches!(type_of(&resolved)?.as_deref(), Some(b"XRef" | b"ObjStm")) {
            return Ok(None);
        }
        out.push(b'S');
        write_dict(&resolved, &mut out, 0, Some(b"Length"))?;
        out.extend_from_slice(&obj.read_raw_stream()?);
        return Ok(Some(out));
    }
    if resolved.is_array()? {
        write(&resolved, &mut out, 0)?;
        return Ok(Some(out));
    }
    if resolved.is_dict()?
        && let Some(kind) = type_of(&resolved)?
        && SHARED_TYPES.contains(&kind.as_slice())
    {
        write(&resolved, &mut out, 0)?;
        return Ok(Some(out));
    }
    Ok(None)
}

fn type_of(dict: &PdfObject) -> Result<Option<Vec<u8>>> {
    Ok(match dict.get_dict("Type")? {
        Some(t) if !t.is_indirect()? && t.is_name()? => Some(t.as_name()?),
        _ => None,
    })
}

/// Deeper than this, an object is not merged (written out as unequal to any other).
const MAX_DEPTH: usize = 32;

/// Writes `obj` out exactly, references as their numbers, a dictionary's keys in order.
fn write(obj: &PdfObject, out: &mut Vec<u8>, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Error::InvalidArgument(
            "an object nested too deep to compare".into(),
        ));
    }
    if obj.is_indirect()? {
        out.extend_from_slice(format!("R{};", obj.as_indirect()?).as_bytes());
    } else if obj.is_null()? {
        out.push(b'n');
    } else if obj.is_bool()? {
        out.push(if obj.as_bool()? { b't' } else { b'f' });
    } else if obj.is_int()? {
        out.extend_from_slice(format!("i{};", obj.as_int()?).as_bytes());
    } else if obj.is_real()? {
        out.extend_from_slice(format!("r{};", obj.as_float()?.to_bits()).as_bytes());
    } else if obj.is_name()? {
        let name = obj.as_name()?;
        out.extend_from_slice(format!("/{}:", name.len()).as_bytes());
        out.extend_from_slice(&name);
    } else if obj.is_string()? {
        let bytes = obj.as_bytes()?;
        out.extend_from_slice(format!("({}:", bytes.len()).as_bytes());
        out.extend_from_slice(&bytes);
    } else if obj.is_array()? {
        out.push(b'[');
        for i in 0..obj.len()? {
            match obj.get_array(i as i32)? {
                Some(item) => write(&item, out, depth + 1)?,
                None => out.push(b'n'),
            }
        }
        out.push(b']');
    } else if obj.is_dict()? {
        write_dict(obj, out, depth, None)?;
    } else {
        out.push(b'?');
    }
    Ok(())
}

fn write_dict(
    dict: &PdfObject,
    out: &mut Vec<u8>,
    depth: usize,
    skip: Option<&[u8]>,
) -> Result<()> {
    let mut entries = Vec::new();
    for i in 0..dict.dict_len()? {
        let Some(key) = dict.get_dict_key(i as i32)? else {
            continue;
        };
        let key = key.as_name()?;
        if skip == Some(key.as_slice()) {
            continue;
        }
        let mut value = Vec::new();
        match dict.get_dict_val(i as i32)? {
            Some(v) => write(&v, &mut value, depth + 1)?,
            None => value.push(b'n'),
        }
        entries.push((key, value));
    }
    entries.sort();
    out.push(b'<');
    for (key, value) in entries {
        out.extend_from_slice(format!("/{}:", key.len()).as_bytes());
        out.extend_from_slice(&key);
        out.extend_from_slice(&value);
    }
    out.push(b'>');
    Ok(())
}

/// Points every reference to a key of `remap` at its value, in every object and the
/// trailer.
fn repoint(doc: &mut PdfDocument, remap: &HashMap<i32, i32>) -> Result<()> {
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    for num in 1..len {
        if remap.contains_key(&num) {
            continue;
        }
        if let Some(mut obj) = doc.new_indirect(num, 0)?.resolve()? {
            repoint_in(doc, &mut obj, remap, 0)?;
        }
    }
    let mut trailer = doc.trailer()?;
    repoint_in(doc, &mut trailer, remap, 0)
}

/// Repoints the references held directly in `obj` (an object's own dictionaries and arrays,
/// not those of the objects it refers to).
fn repoint_in(
    doc: &PdfDocument,
    obj: &mut PdfObject,
    remap: &HashMap<i32, i32>,
    depth: usize,
) -> Result<()> {
    if depth > MAX_DEPTH || obj.is_indirect()? {
        return Ok(());
    }
    if obj.is_dict()? {
        for i in 0..obj.dict_len()? {
            let (Some(key), Some(mut value)) =
                (obj.get_dict_key(i as i32)?, obj.get_dict_val(i as i32)?)
            else {
                continue;
            };
            if value.is_indirect()? {
                if let Some(&to) = remap.get(&value.as_indirect()?) {
                    obj.dict_put(key, doc.new_indirect(to, 0)?)?;
                }
            } else {
                repoint_in(doc, &mut value, remap, depth + 1)?;
            }
        }
    } else if obj.is_array()? {
        for i in 0..obj.len()? {
            let Some(mut item) = obj.get_array(i as i32)? else {
                continue;
            };
            if item.is_indirect()? {
                if let Some(&to) = remap.get(&item.as_indirect()?) {
                    obj.array_put(i as i32, doc.new_indirect(to, 0)?)?;
                }
            } else {
                repoint_in(doc, &mut item, remap, depth + 1)?;
            }
        }
    }
    Ok(())
}

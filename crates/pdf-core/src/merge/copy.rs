//! Copying objects from one document into another.
//!
//! MuPDF's graft map copies everything an object references. That is right for a page's
//! contents and resources, but annotations and links also reference pages (`/P`, `/Dest`),
//! and a page references the page tree (`/Parent`), which references every other page:
//! grafting an annotation would copy the whole source document. MuPDF's own page graft
//! avoids this by leaving annotations out, which loses them.
//!
//! [`Copier`] keeps its own map instead. Before copying, the merge maps every picked
//! source page to its new page, and marks the objects that must never be copied (the
//! catalog, page tree nodes, pages that were not picked and their annotations) as
//! dropped. A reference to a dropped object becomes `null` in arrays and is left out of
//! dictionaries, so nothing can pull in more than the picked pages need.
//!
//! Indirect objects are copied through a queue, not by recursion, so long chains of
//! references in a file cannot overflow the stack.

use std::collections::{HashMap, VecDeque};

use mupdf::Buffer;
use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::Result;
use crate::objects::dict_entries;

/// Nesting of direct arrays and dictionaries inside one object beyond which values are
/// replaced by `null` (only damaged or hostile files nest this deep).
const MAX_DIRECT_DEPTH: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// Copied (or to be copied) as this object number of the destination.
    To(i32),
    /// Never copied.
    Drop,
}

/// Copies objects of one source document into one destination document, each at most
/// once.
#[derive(Default)]
pub(crate) struct Copier {
    map: HashMap<i32, Slot>,
    /// Indirect objects allocated in the destination whose values are not written yet.
    pending: VecDeque<(PdfObject, PdfObject)>,
}

impl Copier {
    pub fn new() -> Copier {
        Copier::default()
    }

    /// Makes references to source object `src` point at destination object `dst`, which
    /// the caller fills in itself (a picked page).
    pub fn map_to(&mut self, src: i32, dst: i32) {
        self.map.insert(src, Slot::To(dst));
    }

    /// Marks source object `src` as never to be copied.
    pub fn drop_object(&mut self, src: i32) {
        self.map.entry(src).or_insert(Slot::Drop);
    }

    /// The destination object number of source object `src`, if it is mapped.
    pub fn mapped(&self, src: i32) -> Option<i32> {
        match self.map.get(&src) {
            Some(Slot::To(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn is_dropped(&self, src: i32) -> bool {
        self.map.get(&src) == Some(&Slot::Drop)
    }

    /// Copies `obj` into `dst`. Returns `None` for a reference to a dropped object.
    /// Indirect objects the copy references are allocated at once and filled in by
    /// [`Copier::finish`].
    pub fn copy(&mut self, dst: &mut PdfDocument, obj: &PdfObject) -> Result<Option<PdfObject>> {
        self.copy_value(dst, obj, 0)
    }

    /// Writes every pending indirect object (which may queue more).
    pub fn finish(&mut self, dst: &mut PdfDocument) -> Result<()> {
        while let Some((src, mut target)) = self.pending.pop_front() {
            let value = match src.resolve()? {
                Some(v) => self.copy_direct(dst, &v, 0, src.is_stream()?)?,
                None => PdfObject::new_null(),
            };
            target.write_object(&value)?;
            if src.is_stream()? {
                // Raw stream data: decrypted but still encoded, so /Filter stays valid.
                let data = src.read_raw_stream()?;
                target.write_raw_stream_buffer(&Buffer::from_copied_bytes(&data)?)?;
            }
        }
        Ok(())
    }

    fn copy_value(
        &mut self,
        dst: &mut PdfDocument,
        obj: &PdfObject,
        depth: u32,
    ) -> Result<Option<PdfObject>> {
        if !obj.is_indirect()? {
            return self.copy_direct(dst, obj, depth, false).map(Some);
        }
        let num = obj.as_indirect()?;
        match self.map.get(&num) {
            Some(Slot::To(n)) => return Ok(Some(dst.new_indirect(*n, 0)?)),
            Some(Slot::Drop) => return Ok(None),
            None => {}
        }
        let target = dst.create_object()?;
        self.map.insert(num, Slot::To(target.as_indirect()?));
        self.pending
            .push_back((obj.try_clone()?, target.try_clone()?));
        Ok(Some(target))
    }

    /// Copies a direct value: arrays and dictionaries element by element, anything else
    /// as it is (numbers, names and strings belong to no document).
    fn copy_direct(
        &mut self,
        dst: &mut PdfDocument,
        obj: &PdfObject,
        depth: u32,
        is_stream: bool,
    ) -> Result<PdfObject> {
        if depth > MAX_DIRECT_DEPTH {
            return Ok(PdfObject::new_null());
        }
        if obj.is_dict()? {
            let mut out =
                dst.new_dict_with_capacity(i32::try_from(obj.dict_len()?).unwrap_or(0))?;
            for (key, value) in dict_entries(obj)? {
                let name = key.as_name()?;
                // The stream's length is set from the copied data.
                if is_stream && name == b"Length" {
                    continue;
                }
                let copied = if name == b"Kids" && value.is_array()? {
                    Some(self.copy_array(dst, &value, depth + 1, true)?)
                } else {
                    self.copy_value(dst, &value, depth + 1)?
                };
                // A dropped object (a page that was not picked) is left out.
                if let Some(copied) = copied {
                    out.dict_put(key, copied)?;
                }
            }
            Ok(out)
        } else if obj.is_array()? {
            self.copy_array(dst, obj, depth, false)
        } else {
            obj.try_clone().map_err(Into::into)
        }
    }

    /// Copies an array. Dropped objects become `null`, or are left out when
    /// `skip_dropped` (field `/Kids`, where a `null` would be a damaged field).
    fn copy_array(
        &mut self,
        dst: &mut PdfDocument,
        obj: &PdfObject,
        depth: u32,
        skip_dropped: bool,
    ) -> Result<PdfObject> {
        let mut out = dst.new_array_with_capacity(i32::try_from(obj.len()?).unwrap_or(0))?;
        for i in 0..i32::try_from(obj.len()?).unwrap_or(i32::MAX) {
            let copied = match obj.get_array(i)? {
                Some(item) => self.copy_value(dst, &item, depth + 1)?,
                // A null element stays null.
                None => Some(PdfObject::new_null()),
            };
            match copied {
                Some(copied) => out.array_push(copied)?,
                None if skip_dropped => {}
                None => out.array_push(PdfObject::new_null())?,
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_shared_objects_once_and_skips_dropped_ones() {
        let mut src = PdfDocument::new();
        let shared = src.new_object_from_str("<< /Kind /Shared >>").unwrap();
        let shared = src.add_object(&shared).unwrap();
        let gone = src.new_object_from_str("<< /Kind /Gone >>").unwrap();
        let dropped = src.add_object(&gone).unwrap();
        let mut holder = src.new_dict().unwrap();
        let mut list = src.new_array().unwrap();
        list.array_push(shared.clone()).unwrap();
        list.array_push(shared.clone()).unwrap();
        list.array_push(dropped.clone()).unwrap();
        holder.dict_put("List", list).unwrap();
        holder.dict_put("Gone", dropped.clone()).unwrap();
        let mut kids = src.new_array().unwrap();
        kids.array_push(dropped.clone()).unwrap();
        kids.array_push(shared.clone()).unwrap();
        holder.dict_put("Kids", kids).unwrap();

        let mut dst = PdfDocument::new();
        let mut copier = Copier::new();
        copier.drop_object(dropped.as_indirect().unwrap());
        let copy = copier.copy(&mut dst, &holder).unwrap().unwrap();
        copier.finish(&mut dst).unwrap();

        let list = copy.get_dict("List").unwrap().unwrap();
        let a = list.get_array(0).unwrap().unwrap();
        let b = list.get_array(1).unwrap().unwrap();
        assert_eq!(a.as_indirect().unwrap(), b.as_indirect().unwrap());
        // MuPDF's null object is a null pointer, which the crate returns as None.
        assert_eq!(list.len().unwrap(), 3);
        assert!(list.get_array(2).unwrap().is_none());
        assert!(copy.get_dict("Gone").unwrap().is_none());
        assert_eq!(copy.get_dict("Kids").unwrap().unwrap().len().unwrap(), 1);
        let kind = a
            .resolve()
            .unwrap()
            .unwrap()
            .get_dict("Kind")
            .unwrap()
            .unwrap();
        assert_eq!(kind.as_name().unwrap(), b"Shared");
    }
}

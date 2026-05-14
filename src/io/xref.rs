// XREF resolution — scan a loaded document for external-reference blocks and
// populate them with geometry from the referenced DWG/DXF files.

use acadrust::entities::{Block, BlockEnd};
use acadrust::types::{Handle, Vector3};
use acadrust::{CadDocument, EntityType};
use std::path::{Path, PathBuf};

/// Status of an external reference block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrefStatus {
    /// File was found and loaded successfully.
    Loaded,
    /// File path is set but the file could not be found or read.
    NotFound,
    /// XRef is marked Unloaded in the host DWG — we honor that and
    /// skip resolving the external file. The user can re-load via UI.
    Unloaded,
}

/// Describes a single external reference found in a document.
#[derive(Debug, Clone)]
pub struct XrefInfo {
    /// Block name (e.g. the filename stem).
    pub name: String,
    /// Resolved file path (or raw path if not found).
    pub path: String,
    pub status: XrefStatus,
}

/// Scan `doc` for XREF block-records, resolve their paths relative to
/// `base_dir`, and populate each xref block with entities from the
/// referenced file.
///
/// Returns a list of [`XrefInfo`] describing each xref block found.
pub fn resolve_xrefs(doc: &mut CadDocument, base_dir: &Path) -> Vec<XrefInfo> {
    // Collect xref blocks: (name, raw_path, block_record_handle)
    //
    // Note: `BlockRecord::is_loaded` (the DWG R2000+ "loaded" bit) turned out
    // to be unreliable as a signal of AutoCAD's Unload state — different
    // files with xrefs the manager calls "Loaded" carry both `Some(true)`
    // and `Some(false)` on disk. Until we identify the actual on-disk
    // representation of XREF→Unload (likely in an XREF dependency record
    // or ACAD_XREF_PANEL_DATA xrecord), we auto-resolve every xref and
    // rely on frustum culling to keep the GPU bounded.
    let xref_entries: Vec<(String, String, Handle)> = doc
        .block_records
        .iter()
        .filter(|br| (br.flags.is_xref || br.flags.is_xref_overlay) && !br.xref_path.is_empty())
        .map(|br| (br.name.clone(), br.xref_path.clone(), br.handle))
        .collect();

    let mut result = Vec::new();

    for (block_name, raw_path, br_handle) in xref_entries {
        let resolved = resolve_path(&raw_path, base_dir);

        let status = match &resolved {
            None => XrefStatus::NotFound,
            Some(p) => match super::load_file(p) {
                Err(_) => XrefStatus::NotFound,
                Ok(xref_doc) => {
                    ensure_block_entities(doc, &block_name);
                    merge_xref_into_block(doc, br_handle, xref_doc);
                    XrefStatus::Loaded
                }
            },
        };

        result.push(XrefInfo {
            name: block_name,
            path: resolved
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(raw_path),
            status,
        });
    }

    result
}

/// Try to build an absolute path from a raw xref path string.
/// Handles absolute paths, relative paths, and Windows-style separators.
fn resolve_path(raw: &str, base_dir: &Path) -> Option<PathBuf> {
    let normalised = raw.replace('\\', "/");
    let p = PathBuf::from(&normalised);

    if p.is_absolute() {
        if p.exists() {
            return Some(p);
        }
        // Fallback: try the filename in base_dir.
        if let Some(fname) = p.file_name() {
            let c = base_dir.join(fname);
            if c.exists() {
                return Some(c);
            }
        }
        return None;
    }

    // Relative path against base_dir.
    let candidate = base_dir.join(&p);
    if candidate.exists() {
        return Some(candidate);
    }

    // Last resort: just the filename.
    if let Some(fname) = p.file_name() {
        let c = base_dir.join(fname);
        if c.exists() {
            return Some(c);
        }
    }

    None
}

/// Make sure `doc` has BLOCK + ENDBLK entities for `block_name`.
/// These are required so renderers can find the block content.
fn ensure_block_entities(doc: &mut CadDocument, block_name: &str) {
    let has_block = doc
        .entities()
        .any(|e| matches!(e, EntityType::Block(b) if b.name == block_name));
    if has_block {
        return;
    }
    let b = Block::new(block_name, Vector3::zero());
    let _ = doc.add_entity(EntityType::Block(b));
    let _ = doc.add_entity(EntityType::BlockEnd(BlockEnd::new()));
}

/// Copy model-space entities from `xref_doc` into the xref block (`br_handle`)
/// of `doc`.
fn merge_xref_into_block(doc: &mut CadDocument, br_handle: Handle, xref_doc: CadDocument) {
    let entities: Vec<EntityType> = xref_doc
        .entities()
        .filter(|e| !matches!(e, EntityType::Block(_) | EntityType::BlockEnd(_)))
        .cloned()
        .collect();

    for mut entity in entities {
        // Clear the foreign handle so acadrust assigns a new one.
        set_handle(&mut entity, Handle::NULL);
        // Route to the xref block record.
        entity.common_mut().owner_handle = br_handle;
        let _ = doc.add_entity(entity);
    }
}

/// Set the handle field of any entity variant.
fn set_handle(entity: &mut EntityType, h: Handle) {
    entity.common_mut().handle = h;
}

// I/O module — open, save, and export CAD documents.
//
// All file reading/writing goes through acadrust.
// Default save format: DWG (AC1032 / R2018+).

pub mod obj;
pub mod pdf_export;
pub mod plot_style;
pub mod print_to_printer;
pub mod step;
pub mod stl;
pub mod xref;

use acadrust::entities::{Dimension, EntityType};
use acadrust::io::dwg::DwgReader;
use acadrust::{CadDocument, DwgWriter, DxfReader, DxfWriter};
use std::path::{Path, PathBuf};

// ── Open ──────────────────────────────────────────────────────────────────

/// Show a file-open dialog and load the selected DWG or DXF file.
/// Returns `(filename, path, document)` or an error string.
pub async fn pick_and_open() -> Result<(String, PathBuf, CadDocument), String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Open CAD file")
        .add_filter("CAD Files", &["dwg", "dxf", "DWG", "DXF"])
        .add_filter("DWG Files", &["dwg", "DWG"])
        .add_filter("DXF Files", &["dxf", "DXF"])
        .add_filter("All Files", &["*"])
        .pick_file()
        .await;

    let handle = match handle {
        Some(h) => h,
        None => return Err("Cancelled".into()),
    };

    let path = handle.path().to_path_buf();
    open_path(path).await
}

/// Load a CAD file from a known path (used by recent files).
pub async fn open_path(path: PathBuf) -> Result<(String, PathBuf, CadDocument), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".into());
    let doc = load_file(&path)?;
    Ok((name, path, doc))
}

/// Load a DWG or DXF file directly from a path (auto-detect by extension).
pub fn load_file(path: &Path) -> Result<CadDocument, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "dwg" => {
            let mut reader = DwgReader::from_file(path).map_err(|e| e.to_string())?;
            reader.read().map_err(|e| e.to_string())
        }
        "dxf" => {
            let mut doc = DxfReader::from_file(path)
                .map_err(|e| e.to_string())?
                .read()
                .map_err(|e| e.to_string())?;
            fix_dxf_dimension_rotations(&mut doc);
            Ok(doc)
        }
        _ => Err(format!("Unsupported file format: .{ext}")),
    }
}

// ── Save dialog ───────────────────────────────────────────────────────────

/// Show a save-file dialog listing all DWG and DXF version filters.
/// DWG versions appear first; format is auto-detected from the returned extension.
pub async fn pick_save_path() -> Option<PathBuf> {
    let dwg_filters: &[(&str, &[&str])] = &[
        ("DWG Files (2018)", &["dwg"]),
        ("DWG Files (2013)", &["dwg"]),
        ("DWG Files (2010)", &["dwg"]),
        ("DWG Files (2007)", &["dwg"]),
        ("DWG Files (2004)", &["dwg"]),
        ("DWG Files (2000)", &["dwg"]),
        ("DWG Files (R14)", &["dwg"]),
        ("DWG Files (R13)", &["dwg"]),
    ];
    let dxf_filters: &[(&str, &[&str])] = &[
        ("DXF Files (2018)", &["dxf"]),
        ("DXF Files (2013)", &["dxf"]),
        ("DXF Files (2010)", &["dxf"]),
        ("DXF Files (2007)", &["dxf"]),
        ("DXF Files (2004)", &["dxf"]),
        ("DXF Files (2000)", &["dxf"]),
        ("DXF Files (R14)", &["dxf"]),
        ("DXF Files (R13)", &["dxf"]),
    ];

    let mut dlg = rfd::AsyncFileDialog::new()
        .set_title("Save As")
        .set_file_name("drawing.dwg");
    for (label, exts) in dwg_filters.iter().chain(dxf_filters.iter()) {
        dlg = dlg.add_filter(*label, *exts);
    }
    dlg.save_file().await.map(|h| h.path().to_path_buf())
}

// ── Plot Style Table ──────────────────────────────────────────────────────

/// Show a file-open dialog and load the selected CTB or STB file.
pub async fn pick_plot_style() -> Option<plot_style::PlotStyleTable> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Load Plot Style Table")
        .add_filter("Plot Style Tables", &["ctb", "stb", "CTB", "STB"])
        .add_filter("CTB Files", &["ctb", "CTB"])
        .add_filter("STB Files", &["stb", "STB"])
        .add_filter("All Files", &["*"])
        .pick_file()
        .await?;
    plot_style::PlotStyleTable::load(handle.path()).ok()
}

// ── Image file picker ─────────────────────────────────────────────────────

/// Show a file-open dialog for raster images and decode the selected file.
/// Returns `(path, pixel_width, pixel_height)` or an error string.
pub async fn pick_image_file() -> Result<(PathBuf, u32, u32), String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("Select Image File")
        .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "tiff", "tif"])
        .add_filter("PNG", &["png"])
        .add_filter("JPEG", &["jpg", "jpeg"])
        .add_filter("All Files", &["*"])
        .pick_file()
        .await
        .ok_or_else(|| "Cancelled".to_string())?;
    let path = handle.path().to_path_buf();
    let img = image::open(&path).map_err(|e| e.to_string())?;
    let (w, h) = image::GenericImageView::dimensions(&img);
    Ok((path, w, h))
}

// ── Save ──────────────────────────────────────────────────────────────────

/// Save the document to the given path.
/// Format is auto-detected from the extension (dwg / dxf).
pub fn save(doc: &CadDocument, path: &Path) -> Result<(), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "dxf" => save_dxf(doc, path),
        _ => save_dwg(doc, path),
    }
}

pub fn save_dwg(doc: &CadDocument, path: &Path) -> Result<(), String> {
    DwgWriter::write_to_file(path, doc).map_err(|e| e.to_string())
}

pub fn save_dxf(doc: &CadDocument, path: &Path) -> Result<(), String> {
    DxfWriter::new(doc)
        .write_to_file(path)
        .map_err(|e| e.to_string())
}

// ── DXF post-load fixups ──────────────────────────────────────────────────

/// The acadrust DXF reader stores LinearDimension.rotation directly from DXF
/// group code 50, which is in degrees.  DWG and our own dimension-creation code
/// store it in radians.  Convert so tessellation can call cos/sin uniformly.
fn fix_dxf_dimension_rotations(doc: &mut CadDocument) {
    for entity in doc.entities_mut() {
        if let EntityType::Dimension(Dimension::Linear(d)) = entity {
            d.rotation = d.rotation.to_radians();
        }
    }
}

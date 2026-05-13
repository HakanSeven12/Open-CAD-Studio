// One-off inspector for diagnosing a DWG: dumps every LwPolyline/Polyline2D/
// Polyline3D with its elevation, normal vector, and per-vertex coordinates.
// Run with:  cargo run --example inspect_polyline -- <path-to-dwg>

use acadrust::entities::EntityType;
use acadrust::io::dwg::DwgReader;
use acadrust::DxfReader;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: inspect_polyline <file>")?;
    let mut doc = if path.to_lowercase().ends_with(".dxf") {
        DxfReader::from_file(&path)?.read()?
    } else {
        DwgReader::from_file(&path)?.read()?
    };

    // Cheap inline corruption check — mirrors the rules in src/io/mod.rs so the
    // example reports the same drop count without needing a library crate.
    let total = doc.entities().count();
    let bad = doc
        .entities()
        .filter(|e| {
            use acadrust::entities::EntityType as E;
            const MAX: usize = 100_000;
            let bad_normal = |n: &acadrust::types::Vector3| {
                !n.x.is_finite()
                    || !n.y.is_finite()
                    || !n.z.is_finite()
                    || ((n.x * n.x + n.y * n.y + n.z * n.z) - 1.0).abs() >= 0.21
            };
            match e {
                E::LwPolyline(p) => {
                    bad_normal(&p.normal)
                        || p.vertices.len() > MAX
                        || !p.elevation.is_finite()
                        || p.elevation.abs() > 1e10
                }
                E::Polyline2D(p) => {
                    bad_normal(&p.normal)
                        || p.vertices.len() > MAX
                        || !p.elevation.is_finite()
                        || p.elevation.abs() > 1e10
                }
                _ => false,
            }
        })
        .count();
    println!("Entities: total={}  corrupt={}", total, bad);
    println!();
    let _ = &mut doc; // silence unused-mut warning if we don't mutate below

    let h = &doc.header;
    println!(
        "Header extents: min=({:.3},{:.3},{:.3}) max=({:.3},{:.3},{:.3})",
        h.model_space_extents_min.x,
        h.model_space_extents_min.y,
        h.model_space_extents_min.z,
        h.model_space_extents_max.x,
        h.model_space_extents_max.y,
        h.model_space_extents_max.z,
    );
    println!();

    let mut lwp = 0usize;
    let mut p2d = 0usize;
    let mut p3d = 0usize;
    let mut pl = 0usize;

    for (i, e) in doc.entities().enumerate() {
        match e {
            EntityType::LwPolyline(p) => {
                lwp += 1;
                if p.common.layer != "PLN_Sev" {
                    continue;
                }
                let nx_abs = p.normal.x.abs();
                let ny_abs = p.normal.y.abs();
                let corrupt = !nx_abs.is_finite()
                    || nx_abs > 1.0
                    || ny_abs > 1.0
                    || p.vertices.len() > 1000;
                if corrupt {
                    println!(
                        "[{}] CORRUPT  handle={:?}  verts={}  elev={:.3e}  normal=({:.2e},{:.2e},{:.2e})",
                        i,
                        p.common.handle,
                        p.vertices.len(),
                        p.elevation,
                        p.normal.x,
                        p.normal.y,
                        p.normal.z,
                    );
                } else {
                    println!(
                        "[{}] OK  handle={:?}  verts={}  elev={:.3}  normal=({:.4},{:.4},{:.4})",
                        i,
                        p.common.handle,
                        p.vertices.len(),
                        p.elevation,
                        p.normal.x,
                        p.normal.y,
                        p.normal.z,
                    );
                    for (vi, v) in p.vertices.iter().take(3).enumerate() {
                        println!("    v{}: ({:.3}, {:.3})  bulge={}", vi, v.location.x, v.location.y, v.bulge);
                    }
                }
            }
            EntityType::Polyline2D(p) => {
                p2d += 1;
                if p2d <= 5 {
                    println!(
                        "[{}] Polyline2D  handle={:?}  layer={:?}",
                        i, p.common.handle, p.common.layer
                    );
                    println!(
                        "    elevation={}  thickness={}  normal=({},{},{})",
                        p.elevation, p.thickness, p.normal.x, p.normal.y, p.normal.z
                    );
                    println!("    closed={}  verts={}", p.is_closed(), p.vertices.len());
                    for (vi, v) in p.vertices.iter().enumerate() {
                        println!(
                            "      v{}: ({}, {}, {})  bulge={}",
                            vi, v.location.x, v.location.y, v.location.z, v.bulge
                        );
                    }
                    println!();
                }
            }
            EntityType::Polyline3D(p) => {
                p3d += 1;
                if p3d <= 5 {
                    println!(
                        "[{}] Polyline3D  handle={:?}  layer={:?}",
                        i, p.common.handle, p.common.layer
                    );
                    println!("    closed={}  verts={}", p.is_closed(), p.vertices.len());
                    for (vi, v) in p.vertices.iter().enumerate() {
                        println!(
                            "      v{}: ({}, {}, {})",
                            vi, v.position.x, v.position.y, v.position.z
                        );
                    }
                    println!();
                }
            }
            EntityType::Polyline(p) => {
                pl += 1;
                if pl <= 5 {
                    println!(
                        "[{}] Polyline (heavy 3D)  handle={:?}  layer={:?}",
                        i, p.common.handle, p.common.layer
                    );
                    println!("    closed={}  verts={}", p.flags.is_closed(), p.vertices.len());
                    for (vi, v) in p.vertices.iter().enumerate() {
                        println!(
                            "      v{}: ({}, {}, {})",
                            vi, v.location.x, v.location.y, v.location.z
                        );
                    }
                    println!();
                }
            }
            _ => {}
        }
    }

    println!(
        "Totals: LwPolyline={}  Polyline2D={}  Polyline3D={}  Polyline={}",
        lwp, p2d, p3d, pl
    );
    Ok(())
}

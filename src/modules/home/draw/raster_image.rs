// IMAGE / IMAGEATTACH command — place a raster image in the drawing.
//
// Workflow:
//   1. File dialog opens (async, handled in update.rs).
//   2. User picks insertion point (first click).
//   3. User drags to pick width; height is computed from the image's aspect ratio.
//   4. Entity is committed.

use acadrust::entities::RasterImage;
use acadrust::types::Vector3;
use acadrust::EntityType;
use glam::Vec3;

use crate::command::{CadCommand, CmdResult};
use crate::scene::wire_model::WireModel;

pub struct ImageCommand {
    file_path: String,
    pixel_width: u32,
    pixel_height: u32,
    origin: Option<Vec3>,
}

impl ImageCommand {
    pub fn new(file_path: String, pixel_width: u32, pixel_height: u32) -> Self {
        Self { file_path, pixel_width, pixel_height, origin: None }
    }

    fn aspect(&self) -> f64 {
        if self.pixel_height == 0 {
            1.0
        } else {
            self.pixel_width as f64 / self.pixel_height as f64
        }
    }

    fn make_entity(&self, origin: Vec3, width_pt: Vec3) -> EntityType {
        let world_width = ((width_pt.x - origin.x) as f64).abs().max(0.001);
        let world_height = world_width / self.aspect();

        let ins = Vector3::new(origin.x as f64, origin.z as f64, origin.y as f64);

        let mut img = RasterImage::with_size(
            &self.file_path,
            ins,
            self.pixel_width as f64,
            self.pixel_height as f64,
            world_width,
            world_height,
        );
        img.flags = acadrust::entities::ImageDisplayFlags::SHOW_IMAGE
            | acadrust::entities::ImageDisplayFlags::USE_CLIPPING_BOUNDARY;
        EntityType::RasterImage(img)
    }
}

impl CadCommand for ImageCommand {
    fn name(&self) -> &'static str {
        "IMAGE"
    }

    fn prompt(&self) -> String {
        if self.origin.is_none() {
            format!("IMAGE  Specify insertion point ({}):  ", short_name(&self.file_path))
        } else {
            "IMAGE  Specify width (drag right):".into()
        }
    }

    fn on_point(&mut self, pt: Vec3) -> CmdResult {
        if let Some(origin) = self.origin {
            let entity = self.make_entity(origin, pt);
            CmdResult::CommitAndExit(entity)
        } else {
            self.origin = Some(pt);
            CmdResult::NeedPoint
        }
    }

    fn on_enter(&mut self) -> CmdResult {
        // If origin is set, place with a default width of 1 unit * pixel count / 100
        if let Some(origin) = self.origin {
            let default_w = (self.pixel_width as f64 / 100.0).max(1.0);
            let width_pt = Vec3::new(origin.x + default_w as f32, origin.y, origin.z);
            let entity = self.make_entity(origin, width_pt);
            CmdResult::CommitAndExit(entity)
        } else {
            CmdResult::Cancel
        }
    }

    fn on_mouse_move(&mut self, pt: Vec3) -> Option<WireModel> {
        let origin = self.origin?;
        let world_width = (pt.x - origin.x).abs().max(0.001);
        let world_height = world_width / self.aspect() as f32;

        let p0 = [origin.x, origin.y, origin.z];
        let p1 = [origin.x + world_width, origin.y, origin.z];
        let p2 = [origin.x + world_width, origin.y + world_height, origin.z];
        let p3 = [origin.x, origin.y + world_height, origin.z];

        Some(WireModel {
            name: "image_preview".into(),
            points: vec![p0, p1, p2, p3, p0],
            color: WireModel::CYAN,
            selected: false,
            pattern_length: 0.0,
            pattern: [0.0; 8],
            line_weight_px: 1.0,
            snap_pts: vec![],
            tangent_geoms: vec![],
            aci: 0,
            key_vertices: vec![],
            aabb: WireModel::UNBOUNDED_AABB,
            plinegen: true,
            vp_scissor: None,
            fill_tris: vec![],
        })
    }
}

fn short_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

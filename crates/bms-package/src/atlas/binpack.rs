use std::collections::BTreeMap;

/// A simple 2D integer rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }
}

/// Placement information for a packed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Result of 2D Bin Packing.
#[derive(Debug, Clone)]
pub struct PackedAtlas {
    pub width: u32,
    pub height: u32,
    pub rects: BTreeMap<String, PackedRect>,
}

/// Zero-crate, lightweight 2D Guillotine rectangle bin packer.
/// Deterministically packs arbitrary rectangular sprites into a minimal power-of-two texture canvas.
pub struct GuillotineBinPacker {
    padding: u32,
}

impl GuillotineBinPacker {
    pub fn new(padding: u32) -> Self {
        Self { padding }
    }

    /// Packs a set of named rectangles `(key, width, height)` into an atlas canvas.
    /// Automatically expands the canvas (up to 4096x4096 or larger) until all items fit.
    pub fn pack(&self, items: &[(String, u32, u32)]) -> Option<PackedAtlas> {
        if items.is_empty() {
            return Some(PackedAtlas {
                width: 1,
                height: 1,
                rects: BTreeMap::new(),
            });
        }

        // Sort items deterministically: Primary by Height (descending), Secondary by Width (desc), Tertiary by Key
        let mut sorted_indices: Vec<usize> = (0..items.len()).collect();
        sorted_indices.sort_by(|&a, &b| {
            let item_a = &items[a];
            let item_b = &items[b];
            item_b
                .2
                .cmp(&item_a.2)
                .then_with(|| item_b.1.cmp(&item_a.1))
                .then_with(|| item_a.0.cmp(&item_b.0))
        });

        // Compute minimum total area required
        let total_area: u64 = items
            .iter()
            .map(|(_, w, h)| (*w as u64 + self.padding as u64 * 2) * (*h as u64 + self.padding as u64 * 2))
            .sum();

        // Start with smallest power-of-two size that could hold the total area
        let mut dim = 64u32;
        while ((dim as u64) * (dim as u64)) < total_area {
            dim *= 2;
        }

        // Also ensure dimension is at least as large as the largest individual item
        for (_, w, h) in items {
            while dim < (*w + self.padding * 2) || dim < (*h + self.padding * 2) {
                dim *= 2;
            }
        }

        // Try packing; if it fails, double the canvas size up to max 8192
        let max_dim = 8192;
        while dim <= max_dim {
            if let Some(rects) = self.try_pack_into(items, &sorted_indices, dim, dim) {
                return Some(PackedAtlas {
                    width: dim,
                    height: dim,
                    rects,
                });
            }
            dim *= 2;
        }

        None
    }

    fn try_pack_into(
        &self,
        items: &[(String, u32, u32)],
        sorted_indices: &[usize],
        canvas_w: u32,
        canvas_h: u32,
    ) -> Option<BTreeMap<String, PackedRect>> {
        let mut free_rects = vec![Rect::new(0, 0, canvas_w, canvas_h)];
        let mut placed: BTreeMap<String, PackedRect> = BTreeMap::new();

        for &idx in sorted_indices {
            let (key, w, h) = &items[idx];
            let needed_w = *w + self.padding * 2;
            let needed_h = *h + self.padding * 2;

            // Find best free rectangle (Best-Short-Side-Fit: minimizes remaining smaller dimension)
            let mut best_idx = None;
            let mut best_short_side = u32::MAX;
            let mut best_area = u64::MAX;

            for (i, free) in free_rects.iter().enumerate() {
                if free.w >= needed_w && free.h >= needed_h {
                    let leftover_w = free.w - needed_w;
                    let leftover_h = free.h - needed_h;
                    let short_side = leftover_w.min(leftover_h);
                    let area = (free.w as u64) * (free.h as u64);

                    if short_side < best_short_side || (short_side == best_short_side && area < best_area) {
                        best_idx = Some(i);
                        best_short_side = short_side;
                        best_area = area;
                    }
                }
            }

            let Some(free_i) = best_idx else {
                return None; // Could not fit this item
            };

            let target_rect = free_rects.remove(free_i);

            // Record placed coordinate (offset by padding)
            placed.insert(
                key.clone(),
                PackedRect {
                    x: target_rect.x + self.padding,
                    y: target_rect.y + self.padding,
                    width: *w,
                    height: *h,
                },
            );

            // Guillotine split of remaining area
            let right_w = target_rect.w - needed_w;
            let bottom_h = target_rect.h - needed_h;

            // Split maximizing smaller rectangle area
            if right_w > 0 && bottom_h > 0 {
                if right_w * target_rect.h > target_rect.w * bottom_h {
                    // Split horizontally first
                    free_rects.push(Rect::new(
                        target_rect.x + needed_w,
                        target_rect.y,
                        right_w,
                        target_rect.h,
                    ));
                    free_rects.push(Rect::new(
                        target_rect.x,
                        target_rect.y + needed_h,
                        needed_w,
                        bottom_h,
                    ));
                } else {
                    // Split vertically first
                    free_rects.push(Rect::new(
                        target_rect.x,
                        target_rect.y + needed_h,
                        target_rect.w,
                        bottom_h,
                    ));
                    free_rects.push(Rect::new(
                        target_rect.x + needed_w,
                        target_rect.y,
                        right_w,
                        needed_h,
                    ));
                }
            } else if right_w > 0 {
                free_rects.push(Rect::new(
                    target_rect.x + needed_w,
                    target_rect.y,
                    right_w,
                    target_rect.h,
                ));
            } else if bottom_h > 0 {
                free_rects.push(Rect::new(
                    target_rect.x,
                    target_rect.y + needed_h,
                    target_rect.w,
                    bottom_h,
                ));
            }
        }

        Some(placed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guillotine_packer_simple() {
        let packer = GuillotineBinPacker::new(1);
        let items = vec![
            ("img1".to_string(), 64, 64),
            ("img2".to_string(), 32, 32),
            ("img3".to_string(), 32, 64),
        ];

        let packed = packer.pack(&items).expect("packing failed");
        assert!(packed.width >= 128);
        assert!(packed.height >= 128);
        assert_eq!(packed.rects.len(), 3);

        // Check non-overlapping
        let r1 = packed.rects.get("img1").unwrap();
        let r2 = packed.rects.get("img2").unwrap();
        let r3 = packed.rects.get("img3").unwrap();

        assert_eq!(r1.width, 64);
        assert_eq!(r1.height, 64);
        assert_eq!(r2.width, 32);
        assert_eq!(r3.width, 32);

        // Verify none of the rectangles overlap (with 1px padding considered)
        let rects = [r1, r2, r3];
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                let a = rects[i];
                let b = rects[j];
                let overlap_x = a.x < b.x + b.width && a.x + a.width > b.x;
                let overlap_y = a.y < b.y + b.height && a.y + a.height > b.y;
                assert!(!(overlap_x && overlap_y), "Rectangles {:?} and {:?} overlap!", a, b);
            }
        }
    }
}

//! Damage-Region & Multi-Surface Z-Order Compositor (Feature 44).
//!
//! Manages bounded application surfaces, owner-slot isolation, focus, Z-order,
//! visibility, and merged dirty-rectangle damage tracking for partial redraws.

pub const MAX_SURFACES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl DamageRect {
    pub const EMPTY: Self = Self { x: 0, y: 0, w: 0, h: 0 };

    pub const fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn union(&self, other: DamageRect) -> DamageRect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return *self;
        }
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w).max(other.x + other.w);
        let y1 = (self.y + self.h).max(other.y + other.h);
        DamageRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    pub fn clip(&self, bounds: DamageRect) -> DamageRect {
        if self.is_empty() || bounds.is_empty() {
            return Self::EMPTY;
        }
        let x0 = self.x.max(bounds.x);
        let y0 = self.y.max(bounds.y);
        let x1 = (self.x + self.w).min(bounds.x + bounds.w);
        let y1 = (self.y + self.h).min(bounds.y + bounds.h);
        if x1 <= x0 || y1 <= y0 {
            Self::EMPTY
        } else {
            DamageRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Surface {
    pub in_use: bool,
    pub id: u32,
    pub owner_slot: u32,
    pub z_order: u16,
    pub visible: bool,
    pub bounds: DamageRect,
    pub damage: DamageRect,
}

impl Surface {
    pub const EMPTY: Self = Self {
        in_use: false,
        id: 0,
        owner_slot: 0,
        z_order: 0,
        visible: false,
        bounds: DamageRect::EMPTY,
        damage: DamageRect::EMPTY,
    };
}

pub struct Compositor {
    screen: DamageRect,
    surfaces: [Surface; MAX_SURFACES],
    next_id: u32,
    focused_surface: Option<u32>,
    frame_damage: DamageRect,
}

impl Compositor {
    pub const fn new(screen_w: i32, screen_h: i32) -> Self {
        Self {
            screen: DamageRect { x: 0, y: 0, w: screen_w, h: screen_h },
            surfaces: [Surface::EMPTY; MAX_SURFACES],
            next_id: 1,
            focused_surface: None,
            frame_damage: DamageRect::EMPTY,
        }
    }

    pub fn create_surface(&mut self, owner_slot: u32, bounds: DamageRect, z_order: u16) -> Option<u32> {
        let slot = self.surfaces.iter_mut().find(|s| !s.in_use)?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        *slot = Surface {
            in_use: true,
            id,
            owner_slot,
            z_order,
            visible: true,
            bounds,
            damage: bounds.clip(self.screen),
        };
        self.frame_damage = self.frame_damage.union(slot.damage);
        self.focused_surface = Some(id);
        Some(id)
    }

    pub fn mark_damage(&mut self, caller_slot: u32, surface_id: u32, rect: DamageRect) -> bool {
        let Some(surf) = self.surfaces.iter_mut().find(|s| s.in_use && s.id == surface_id) else {
            return false;
        };
        if surf.owner_slot != caller_slot {
            return false;
        }
        let clipped = rect.clip(surf.bounds).clip(self.screen);
        surf.damage = surf.damage.union(clipped);
        if surf.visible {
            self.frame_damage = self.frame_damage.union(clipped);
        }
        true
    }

    pub fn destroy_surface(&mut self, caller_slot: u32, surface_id: u32) -> bool {
        let Some(surf) = self.surfaces.iter_mut().find(|s| s.in_use && s.id == surface_id) else {
            return false;
        };
        if surf.owner_slot != caller_slot {
            return false;
        }
        self.frame_damage = self.frame_damage.union(surf.bounds.clip(self.screen));
        *surf = Surface::EMPTY;
        if self.focused_surface == Some(surface_id) {
            self.focused_surface = None;
        }
        true
    }

    /// Take the accumulated screen damage rect for partial frame composition and clear it.
    pub fn take_frame_damage(&mut self) -> DamageRect {
        let d = self.frame_damage;
        self.frame_damage = DamageRect::EMPTY;
        for s in self.surfaces.iter_mut() {
            if s.in_use {
                s.damage = DamageRect::EMPTY;
            }
        }
        d
    }

    pub fn focused(&self) -> Option<u32> {
        self.focused_surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_isolates_surface_owners_and_merges_damage_regions() {
        let mut comp = Compositor::new(540, 960);
        let s1 = comp.create_surface(1, DamageRect { x: 0, y: 0, w: 540, h: 960 }, 1).unwrap();
        let _ = comp.take_frame_damage();

        // Unauthorized task 2 cannot damage or destroy task 1's surface.
        assert!(!comp.mark_damage(2, s1, DamageRect { x: 10, y: 10, w: 40, h: 40 }));
        assert!(!comp.destroy_surface(2, s1));

        // Owner marks two small rects -> merged into tight bounding box.
        assert!(comp.mark_damage(1, s1, DamageRect { x: 10, y: 20, w: 30, h: 40 }));
        assert!(comp.mark_damage(1, s1, DamageRect { x: 100, y: 50, w: 20, h: 30 }));
        assert_eq!(comp.take_frame_damage(), DamageRect { x: 10, y: 20, w: 110, h: 60 });
        assert!(comp.take_frame_damage().is_empty());
    }
}

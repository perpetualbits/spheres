//! Camera.
//!
//! The camera rests at `CAMERA_REST_DIST` on +Z, looking down -Z at the world
//! centre where the child spheres are clustered. During an eversion it only
//! bobs forward slightly (zero at both ends of the gesture), keeping a stable
//! reference frame — the enveloping is carried by the target shell growing past
//! the eye, not by flinging the camera around. That stability is deliberate
//! anti-nausea design.

use glam::{Mat4, Vec3, Vec4};

use crate::config;

pub struct Camera {
    /// Aspect ratio (width / height), updated on resize.
    pub aspect: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Camera { aspect }
    }

    /// Eye position for a given eased eversion progress.
    pub fn eye(&self, eased_t: f32) -> Vec3 {
        // A small forward push that returns to rest at both ends (sin is 0 at
        // 0 and 1), so resting frames are identical before and after.
        let bob = config::CAMERA_PUSH * (std::f32::consts::PI * eased_t).sin();
        Vec3::new(0.0, 0.0, config::CAMERA_REST_DIST - bob)
    }

    fn projection(&self) -> Mat4 {
        Mat4::perspective_rh(
            config::CAMERA_FOV_DEG.to_radians(),
            self.aspect.max(0.0001),
            config::CAMERA_NEAR,
            config::CAMERA_FAR,
        )
    }

    fn view(&self, eased_t: f32) -> Mat4 {
        let eye = self.eye(eased_t);
        let center = eye - Vec3::Z; // always look along -Z
        Mat4::look_at_rh(eye, center, Vec3::Y)
    }

    /// Combined view-projection matrix for the current eased progress.
    pub fn view_proj(&self, eased_t: f32) -> Mat4 {
        self.projection() * self.view(eased_t)
    }

    /// Build a world-space ray from normalised device coordinates
    /// (`ndc` in [-1, 1], y up). Used to pick the sphere under the cursor.
    /// Uses the resting pose, since picking only happens while resting.
    pub fn ray(&self, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(0.0).inverse();
        let eye = self.eye(0.0);
        // Unproject a point on the far plane (wgpu NDC depth is [0, 1]).
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let far = far.truncate() / far.w;
        (eye, (far - eye).normalize_or_zero())
    }
}

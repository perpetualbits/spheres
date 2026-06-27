//! Camera.
//!
//! Looks down -Z at the world origin. The eye distance is supplied per frame
//! (it interpolates between the pulled-back overview distance and the closer
//! local-world distance during an eversion — see `Nav::eye_distance`), so the
//! camera itself stays a simple function of one scalar.

use glam::{Mat4, Vec3, Vec4};

use crate::config;

pub struct Camera {
    pub aspect: f32,
}

impl Camera {
    pub fn new(aspect: f32) -> Self {
        Camera { aspect }
    }

    pub fn eye(&self, eye_z: f32) -> Vec3 {
        Vec3::new(0.0, 0.0, eye_z)
    }

    fn projection(&self) -> Mat4 {
        Mat4::perspective_rh(
            config::CAMERA_FOV_DEG.to_radians(),
            self.aspect.max(0.0001),
            config::CAMERA_NEAR,
            config::CAMERA_FAR,
        )
    }

    pub fn view_proj(&self, eye_z: f32) -> Mat4 {
        let eye = self.eye(eye_z);
        let view = Mat4::look_at_rh(eye, eye - Vec3::Z, Vec3::Y);
        self.projection() * view
    }

    /// World-space ray from normalised device coords ([-1,1], y up), for
    /// picking the node under the cursor.
    pub fn ray(&self, eye_z: f32, ndc_x: f32, ndc_y: f32) -> (Vec3, Vec3) {
        let inv = self.view_proj(eye_z).inverse();
        let eye = self.eye(eye_z);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let far = far.truncate() / far.w;
        (eye, (far - eye).normalize_or_zero())
    }
}

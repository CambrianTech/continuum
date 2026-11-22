//! The slice's control law, pluggable: whatever sets how long a mind holds the lane implements
//! [`Control`], and the [`super::Slicer`] around it (shares, the wall, the deadline guard) stays the
//! same. One law can be swapped for another live, compared in the simulator ([`super::sim`]), or
//! replayed against a captured trace ([`super::trace`]).

/// A control law for the slice. It is told every measured step and switch, and at the end of each
/// window (one rotation of the ready minds) whether any mind waited past the bound, and the
/// `[floor, wall]` the slice must stay in (floor: one step; wall: the longest slice that keeps the
/// bound). It answers with the slice it wants now, in seconds.
pub trait Control: Send {
    /// Its name, as a trace and a report label it.
    fn name(&self) -> &'static str;
    /// The slice it wants now, seconds.
    fn slice(&self) -> f64;
    /// The engine stepped once, measured.
    fn stepped(&mut self, seconds: f64);
    /// The engine switched, measured.
    fn switched(&mut self, seconds: f64);
    /// The window closed.
    fn close(&mut self, over_bound: bool, floor: f64, wall: f64);
}

/// A slice that never moves: the baseline every adaptive law is compared against, never a
/// production law (a fixed number where a measurement exists is the defect).
#[derive(Debug, Clone, Copy)]
pub struct Fixed(pub f64);

impl Control for Fixed {
    fn name(&self) -> &'static str {
        "fixed"
    }
    fn slice(&self) -> f64 {
        self.0
    }
    fn stepped(&mut self, _: f64) {}
    fn switched(&mut self, _: f64) {}
    fn close(&mut self, _: bool, _: f64, _: f64) {}
}

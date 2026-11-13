//! Unix has no share mode: a pin is an ordinary read handle, and `mapping` holds no component.

use std::fs::File;
use std::io;
use std::path::Path;

use super::Component;

pub(super) const FORBIDS_REPLACEMENT: bool = false;

pub(super) fn open_file(path: &Path) -> io::Result<File> {
    File::open(path)
}

pub(super) fn hold_component(path: &Path, _role: Component) -> io::Result<File> {
    File::open(path)
}

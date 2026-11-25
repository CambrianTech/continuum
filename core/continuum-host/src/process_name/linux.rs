/// `pid`'s command from `/proc/<pid>/comm`: the field `ps -o comm=` reads.
pub(super) fn name_of(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()
}

/// Linux keeps a zombie in /proc, where the process table reader sees its status; Windows has
/// no zombie (a retained terminated handle is the probe's to read). Nothing to add here.
pub(super) fn exited_unreaped(_pid: u32) -> bool {
    false
}

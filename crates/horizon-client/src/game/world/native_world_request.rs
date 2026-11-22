/// A request the title's next frame would take: the folder, and the mods it is asked for, by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) folder: String,
    pub(crate) mods: String,
}

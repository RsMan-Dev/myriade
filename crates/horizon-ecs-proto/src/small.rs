//! `Nth<T, N>`: the per-entry state of a module that carries several entries of its kind (the pig's two tempt goals), kept in the
//! component itself instead of a heap `Vec` (one allocation less per entity). Indexed by `Inst::nth`, in the order of the entries, with
//! the `Vec` calls the modules use (`push`, `len`, `get`, `get_mut`, `iter`, `[i]`). `N` is the most entries of the module any species
//! carries: `tests/it/inline_bounds.rs` proves it for the registry, and a push beyond it is a configuration error, never a silent drop.
#[derive(Clone, Copy, Debug)]
pub struct Nth<T, const N: usize> {
    items: [Option<T>; N],
    len: u8,
}
impl<T: Copy, const N: usize> Nth<T, N> {
    pub const fn new() -> Self {
        Self { items: [None; N], len: 0 }
    }
    /// A list of one element.
    pub fn one(x: T) -> Self {
        let mut s = Self::new();
        s.push(x);
        s
    }
    pub fn push(&mut self, x: T) {
        assert!((self.len as usize) < N, "more than {N} entries of one module on an entity: raise the capacity of its `Nth`");
        self.items[self.len as usize] = Some(x);
        self.len += 1;
    }
    pub fn len(&self) -> usize {
        self.len as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, i: usize) -> Option<&T> {
        self.items.get(i).and_then(|x| x.as_ref())
    }
    pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
        self.items.get_mut(i).and_then(|x| x.as_mut())
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items[..self.len as usize].iter().map(|x| x.as_ref().expect("filled below len"))
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.items[..self.len as usize].iter_mut().map(|x| x.as_mut().expect("filled below len"))
    }
}
impl<T: Copy, const N: usize> Default for Nth<T, N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: Copy, const N: usize> std::ops::Index<usize> for Nth<T, N> {
    type Output = T;
    fn index(&self, i: usize) -> &T {
        self.get(i).expect("no entry at this nth")
    }
}
impl<T: Copy, const N: usize> std::ops::IndexMut<usize> for Nth<T, N> {
    fn index_mut(&mut self, i: usize) -> &mut T {
        self.get_mut(i).expect("no entry at this nth")
    }
}

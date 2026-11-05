//! The world's action list, in the order the original keeps it (docs/29 section 12ba).
//!
//! `World::stepTo` runs its actions (`0x52a990`: the world's one action, `0x530fd0`, which calls
//! vtable `+0x14` of every entry of its own list at `+0x90..+0x94`, in list order). Those entries
//! are, per robot, its `MagicAction` (`0x4856f0`), its constraint group (`0x523bd0`: the solver,
//! `0x534480`) and one controller per hinge or slider (`0x481180`, `0x4b4e00`: the update with
//! the auto-lock); the arena has a group of its own and a controller per hazard joint; the drag
//! action (`0x52d200`) is the first. The list is a vector of `(action, priority)` entries; every
//! `addAction` (`0x4ad100` -> `0x530700`) appends one (priority always 0) and sorts the vector,
//! every `removeAction` (`0x4ad130` -> `0x530ca0`) erases the first entry of the action and sorts
//! again. The sort is the compiler's `std::sort` over 8-byte entries by priority: quicksort
//! partitions (`0x531710`) while a range holds more than 16 entries, then insertion sorts. With
//! all priorities equal, up to 16 entries it keeps the order (insertion sort); above that every
//! partition swaps its range end for end, so the order of a list of 17 or more is permuted at
//! every add and remove. A Box fight has 17 to 19 entries while the second robot arrives, which
//! is why the Box's ALARM controller runs after the solver and the Compressor's piston before.
//!
//! [`ActionList::order`] after the same calls reproduces all 23 lists recorded live in
//! `boxstart-actorder-*` (`tools/oracle/pressed/al_sort.py` is the same code).

/// The list: `(action, priority)` entries.
#[derive(Clone, Debug, Default)]
pub struct ActionList<T: Copy + PartialEq> {
    entries: Vec<(T, i32)>,
}

impl<T: Copy + PartialEq> ActionList<T> {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// `addAction(action)`: append it with priority 0 and sort.
    pub fn add(&mut self, action: T) {
        self.entries.push((action, 0));
        self.sort();
    }

    /// `removeAction(action)`: erase its first entry and sort.
    pub fn remove(&mut self, action: T) {
        if let Some(i) = self.entries.iter().position(|e| e.0 == action) {
            self.entries.remove(i);
        }
        self.sort();
    }

    /// The actions in the order the world runs them.
    pub fn order(&self) -> Vec<T> {
        self.entries.iter().map(|e| e.0).collect()
    }

    /// Where `action` runs in the list.
    pub fn position(&self, action: T) -> Option<usize> {
        self.entries.iter().position(|e| e.0 == action)
    }

    fn sort(&mut self) {
        let n = self.entries.len();
        let a = &mut self.entries;
        if n <= 16 {
            for i in 1..n {
                insert_guarded(a, i);
            }
            return;
        }
        quick_loop(a, 0, n);
        for i in 1..16 {
            insert_guarded(a, i);
        }
        for i in 16..n {
            insert_unguarded(a, i);
        }
    }
}

/// Insertion of `a[i]` into `a[..i]` (the first element is checked on its own).
fn insert_guarded<T: Copy>(a: &mut [(T, i32)], i: usize) {
    let v = a[i];
    if v.1 < a[0].1 {
        a.copy_within(0..i, 1);
        a[0] = v;
    } else {
        let mut j = i;
        while v.1 < a[j - 1].1 {
            a[j] = a[j - 1];
            j -= 1;
        }
        a[j] = v;
    }
}

/// The same with the front known to be the smallest (`0x5301e7` onwards).
fn insert_unguarded<T: Copy>(a: &mut [(T, i32)], i: usize) {
    let v = a[i];
    let mut j = i;
    while v.1 < a[j - 1].1 {
        a[j] = a[j - 1];
        j -= 1;
    }
    a[j] = v;
}

/// The median of the first, middle and last keys (`0x530700`, `0x531761`).
fn pivot_key<T: Copy>(a: &[(T, i32)], first: usize, last: usize) -> i32 {
    let (ka, kb, kc) = (a[first].1, a[first + (last - first) / 2].1, a[last - 1].1);
    if ka < kb {
        if kb < kc {
            kb
        } else if kc <= ka {
            ka
        } else {
            kc
        }
    } else if ka < kc {
        ka
    } else if kc <= kb {
        kb
    } else {
        kc
    }
}

/// The partition loop (`0x531710`): while the range holds more than 16 entries, split it about
/// the median key, recurse into the smaller part and go on with the larger.
fn quick_loop<T: Copy>(a: &mut [(T, i32)], mut first: usize, mut last: usize) {
    while last - first > 16 {
        let p = pivot_key(a, first, last);
        let (mut i, mut j) = (first, last);
        loop {
            while a[i].1 < p {
                i += 1;
            }
            j -= 1;
            while p < a[j].1 {
                j -= 1;
            }
            if j <= i {
                break;
            }
            a.swap(i, j);
            i += 1;
        }
        if i - first < last - i {
            quick_loop(a, first, i);
            first = i;
        } else {
            quick_loop(a, i, last);
            last = i;
        }
    }
}

/// One entry of the world's list, named by what it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Act {
    /// The drag action.
    Drag,
    /// The arena's constraint group.
    World,
    /// Robot `r`'s `MagicAction`.
    Magic(usize),
    /// Robot `r`'s constraint group.
    Group(usize),
    /// The temporary group a robot's controller starts with, folded into the robot's group later.
    Temp(usize, usize),
    /// Controller `i` of robot `r`.
    Ctl(usize, usize),
    /// Controller `j` of the arena.
    Arena(usize),
}

/// The list at the bell for `robots` (the number of controllers of each, in the order they are
/// placed) and `arena` hazard controllers, from the calls recorded live (docs/29 section 12ba):
/// the drag action and the arena's group; per robot its `MagicAction`, its group and for every
/// controller a temporary group and the controller, the temporary groups removed once the
/// robot has settled (the live removals of all of a robot's groups come before the next robot
/// arrives, 232 to 1553 steps after each); at the bell the arena's controllers.
pub fn match_list(robots: &[usize], arena: usize) -> ActionList<Act> {
    let mut list = ActionList::new();
    list.add(Act::Drag);
    list.add(Act::World);
    for (r, &k) in robots.iter().enumerate() {
        list.add(Act::Magic(r));
        list.add(Act::Group(r));
        for i in 0..k {
            list.add(Act::Temp(r, i));
            list.add(Act::Ctl(r, i));
        }
        for i in 0..k {
            list.remove(Act::Temp(r, i));
        }
    }
    for j in 0..arena {
        list.add(Act::Arena(j));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use Act::*;

    /// The two lists recorded live at the bell (`boxstart-actorder-scout-alarm-box__live__003`,
    /// `boxstart-actorder-sentinel-wideload-compressor__live__001`), by what each entry is.
    #[test]
    fn the_box_list_matches_the_live_one() {
        let want = vec![
            Arena(2),
            Arena(1),
            Arena(0),
            Drag,
            World,
            Magic(0),
            Group(0),
            Ctl(0, 0),
            Ctl(0, 1),
            Ctl(0, 2),
            Magic(1),
            Group(1),
            Ctl(1, 0),
            Ctl(1, 1),
            Ctl(1, 2),
            Ctl(1, 3),
            Ctl(1, 4),
        ];
        assert_eq!(match_list(&[3, 5], 3).order(), want);
    }

    #[test]
    fn the_compressor_list_matches_the_live_one() {
        let mut want = vec![Arena(3), Arena(1)];
        want.extend((0..5).rev().map(|i| Ctl(1, i)));
        want.extend([Group(1), Magic(1)]);
        want.extend((0..5).rev().map(|i| Ctl(0, i)));
        want.extend([Group(0), Magic(0), World, Drag, Arena(0), Arena(2)]);
        assert_eq!(match_list(&[5, 5], 4).order(), want);
    }

    #[test]
    fn up_to_sixteen_entries_keep_their_order() {
        let mut l = ActionList::new();
        for i in 0..16 {
            l.add(i);
        }
        assert_eq!(l.order(), (0..16).collect::<Vec<_>>());
        l.add(16);
        assert_ne!(l.order(), (0..17).collect::<Vec<_>>());
        // Removing the extra entry sorts the list of 16 again, which keeps what it has.
        let held = l.order();
        l.remove(16);
        assert_eq!(
            l.order(),
            held.into_iter().filter(|&x| x != 16).collect::<Vec<_>>()
        );
    }
}

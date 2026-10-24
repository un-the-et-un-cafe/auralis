//! Plumtree epidemic broadcast trees for large communities (PROTOCOL §22.3).
//!
//! Above [`PLUMTREE_MIN_MEMBERS`] members, a post is pushed eagerly to
//! [`EAGER_FANOUT`] peers and announced lazily (id only, `IHAVE`) to the rest.
//! Duplicates prune the sender to lazy; a missing announced id grafts the
//! announcer back into the eager set after [`GRAFT_TIMEOUT_MS`].

use std::collections::{BTreeSet, HashMap, HashSet};

pub const PLUMTREE_MIN_MEMBERS: usize = 256;
pub const EAGER_FANOUT: usize = 3;
pub const GRAFT_TIMEOUT_MS: u64 = 2_000;

pub type PeerId = u32;
pub type MsgId = [u8; 16];

/// Use Plumtree instead of plain fan-out?
pub fn use_plumtree(members: usize) -> bool {
    members > PLUMTREE_MIN_MEMBERS
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Send the full post.
    Push(PeerId, MsgId),
    /// Send only the id.
    IHave(PeerId, MsgId),
    Prune(PeerId),
    Graft(PeerId, MsgId),
    /// Hand the post to the application.
    Deliver(MsgId),
}

#[derive(Debug, Default)]
pub struct Plumtree {
    eager: BTreeSet<PeerId>,
    lazy: BTreeSet<PeerId>,
    seen: HashSet<MsgId>,
    missing: HashMap<MsgId, (PeerId, u64)>,
}

impl Plumtree {
    /// Start with the first [`EAGER_FANOUT`] peers eager, the rest lazy.
    pub fn new(peers: &[PeerId]) -> Self {
        let mut t = Self::default();
        for (i, p) in peers.iter().enumerate() {
            if i < EAGER_FANOUT {
                t.eager.insert(*p);
            } else {
                t.lazy.insert(*p);
            }
        }
        t
    }

    pub fn eager(&self) -> &BTreeSet<PeerId> {
        &self.eager
    }

    fn fanout(&self, id: MsgId, except: Option<PeerId>, out: &mut Vec<Action>) {
        for p in &self.eager {
            if Some(*p) != except {
                out.push(Action::Push(*p, id));
            }
        }
        for p in &self.lazy {
            if Some(*p) != except {
                out.push(Action::IHave(*p, id));
            }
        }
    }

    pub fn broadcast(&mut self, id: MsgId) -> Vec<Action> {
        let mut out = vec![Action::Deliver(id)];
        self.seen.insert(id);
        self.fanout(id, None, &mut out);
        out
    }

    pub fn on_push(&mut self, from: PeerId, id: MsgId) -> Vec<Action> {
        let mut out = Vec::new();
        if self.seen.insert(id) {
            self.missing.remove(&id);
            self.lazy.remove(&from);
            self.eager.insert(from);
            out.push(Action::Deliver(id));
            self.fanout(id, Some(from), &mut out);
        } else {
            self.eager.remove(&from);
            self.lazy.insert(from);
            out.push(Action::Prune(from));
        }
        out
    }

    pub fn on_ihave(&mut self, from: PeerId, id: MsgId, now_ms: u64) {
        if !self.seen.contains(&id) {
            self.missing
                .entry(id)
                .or_insert((from, now_ms + GRAFT_TIMEOUT_MS));
        }
    }

    pub fn on_prune(&mut self, from: PeerId) {
        self.eager.remove(&from);
        self.lazy.insert(from);
    }

    pub fn on_graft(&mut self, from: PeerId, id: MsgId) -> Vec<Action> {
        self.lazy.remove(&from);
        self.eager.insert(from);
        if self.seen.contains(&id) {
            vec![Action::Push(from, id)]
        } else {
            vec![]
        }
    }

    /// Fire graft timers.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Action> {
        let due: Vec<MsgId> = self
            .missing
            .iter()
            .filter(|(_, (_, t))| *t <= now_ms)
            .map(|(k, _)| *k)
            .collect();
        let mut out = Vec::new();
        for id in due {
            let (p, _) = self.missing.remove(&id).expect("present");
            self.lazy.remove(&p);
            self.eager.insert(p);
            out.push(Action::Graft(p, id));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eager_lazy_prune_graft() {
        assert!(!use_plumtree(256));
        assert!(use_plumtree(257));
        let mut t = Plumtree::new(&[1, 2, 3, 4, 5]);
        let a = t.broadcast([1; 16]);
        assert_eq!(
            a.iter().filter(|x| matches!(x, Action::Push(..))).count(),
            3
        );
        assert_eq!(
            a.iter().filter(|x| matches!(x, Action::IHave(..))).count(),
            2
        );
        // duplicate from 2 → prune.
        assert_eq!(t.on_push(2, [1; 16]), vec![Action::Prune(2)]);
        assert!(!t.eager().contains(&2));
        // IHAVE for unknown id → graft after timeout.
        t.on_ihave(5, [2; 16], 0);
        assert!(t.tick(1_999).is_empty());
        assert_eq!(t.tick(2_000), vec![Action::Graft(5, [2; 16])]);
        assert!(t.eager().contains(&5));
        assert_eq!(t.on_graft(4, [1; 16]), vec![Action::Push(4, [1; 16])]);
        t.on_prune(4);
        assert!(!t.eager().contains(&4));
    }
}

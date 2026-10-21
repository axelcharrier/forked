//! Lane layout for the commit graph.
//!
//! Each commit row gets a node lane plus the edges crossing it. Lanes are
//! reused but never compacted, so a pass-through edge stays in the same column
//! for the whole row and the renderer only needs per-row data.

use super::{CommitInfo, Oid};

#[derive(Clone, Debug, Default)]
pub struct GraphRow {
    pub lane: usize,
    /// Lanes that continue straight through this row (top to bottom).
    pub pass_through: Vec<usize>,
    /// Lanes that arrive from the top and end at the node.
    pub incoming: Vec<usize>,
    /// Lanes that leave the node towards the bottom (one per parent).
    pub outgoing: Vec<usize>,
    pub width: usize,
}

pub fn layout(commits: &[CommitInfo]) -> Vec<GraphRow> {
    // Each active lane holds the commit it is waiting for.
    let mut lanes: Vec<Option<Oid>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());

    for c in commits {
        let mut row = GraphRow::default();

        let waiting: Vec<usize> = lanes
            .iter()
            .enumerate()
            .filter(|(_, l)| **l == Some(c.id))
            .map(|(i, _)| i)
            .collect();

        let lane = match waiting.first() {
            Some(&i) => i,
            None => alloc(&mut lanes),
        };
        row.lane = lane;
        row.incoming = waiting.clone();

        for (i, l) in lanes.iter().enumerate() {
            if l.is_some() && *l != Some(c.id) {
                row.pass_through.push(i);
            }
        }

        // The branches that merged into this commit end here.
        for &i in &waiting {
            lanes[i] = None;
        }

        for (pi, parent) in c.parents.iter().enumerate() {
            // A parent another lane is already heading to joins that lane;
            // otherwise the first parent continues in the node's own lane.
            let target = match lanes.iter().position(|l| *l == Some(*parent)) {
                Some(existing) => existing,
                None => {
                    let i = if pi == 0 { lane } else { alloc(&mut lanes) };
                    lanes[i] = Some(*parent);
                    i
                }
            };
            row.outgoing.push(target);
        }

        let widest_edge = row
            .pass_through
            .iter()
            .chain(&row.incoming)
            .chain(&row.outgoing)
            .max()
            .map_or(0, |m| m + 1);
        row.width = widest_edge.max(lane + 1);
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        rows.push(row);
    }
    rows
}

fn alloc(lanes: &mut Vec<Option<Oid>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(i) => i,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: u8, parents: &[u8]) -> CommitInfo {
        let oid = |b: u8| Oid::from_bytes(&[b; 20]).unwrap();
        CommitInfo {
            id: oid(id),
            short_id: String::new(),
            summary: String::new(),
            message: String::new(),
            author: String::new(),
            email: String::new(),
            time: 0,
            committer: String::new(),
            commit_time: 0,
            parents: parents.iter().map(|p| oid(*p)).collect(),
            refs: Vec::new(),
        }
    }

    #[test]
    fn linear_history_stays_in_one_lane() {
        let rows = layout(&[commit(3, &[2]), commit(2, &[1]), commit(1, &[])]);
        assert!(rows.iter().all(|r| r.lane == 0 && r.pass_through.is_empty()));
    }

    #[test]
    fn merge_opens_and_closes_a_lane() {
        // 4 = merge(3, 2); 3 -> 1; 2 -> 1
        let rows = layout(&[
            commit(4, &[3, 2]),
            commit(3, &[1]),
            commit(2, &[1]),
            commit(1, &[]),
        ]);
        assert_eq!(rows[0].outgoing, vec![0, 1]);
        assert_eq!(rows[1].lane, 0);
        assert_eq!(rows[1].pass_through, vec![1]);
        assert_eq!(rows[2].lane, 1);
        // 2's parent (1) is already awaited by lane 0, so it joins lane 0.
        assert_eq!(rows[2].outgoing, vec![0]);
        assert_eq!(rows[3].lane, 0);
        assert_eq!(rows[3].incoming, vec![0]);
    }

    #[test]
    fn unrelated_branch_tips_get_their_own_lanes() {
        // Two tips (5 and 4) sharing ancestor 1.
        let rows = layout(&[commit(5, &[1]), commit(4, &[1]), commit(1, &[])]);
        assert_eq!(rows[0].lane, 0);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[1].outgoing, vec![0]);
        assert_eq!(rows[2].incoming, vec![0]);
    }
}

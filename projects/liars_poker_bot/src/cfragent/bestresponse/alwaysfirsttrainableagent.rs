use crate::{
    cfragent::CFRNode,
    database::NodeStore,
    game::{Game, GameState},
};

/// Populates a nodestore to always pick a given action index
pub(super) fn _populate_always_n<T: GameState, N: NodeStore>(ns: &mut N, g: Game<T>, idx: usize) {
    for _ in 0..100 {
        let gs = (g.new)();
        let mut q = Vec::new();
        q.push(gs);

        while let Some(gs) = q.pop() {
            if gs.is_terminal() {
                continue;
            }

            if !gs.is_chance_node() {
                let p = gs.cur_player();
                let k = gs.istate_key(p);
                let mut node = CFRNode::new(k, &gs.legal_actions());
                node.total_move_prob[idx] = 1.0; // set the moveprob to 1 for the action of the target index
                ns.insert_node(k, node);
            }

            for a in gs.legal_actions() {
                let mut ngs = gs.clone();
                ngs.apply_action(a);
                q.push(ngs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        database::{memory_node_store::MemoryNodeStore, NodeStore},
        game::GameState,
        kuhn_poker::KuhnPoker,
    };

    use super::_populate_always_n;

    #[test]
    fn test_populate_ns() {
        let mut ns = MemoryNodeStore::new();
        let g = KuhnPoker::game();
        _populate_always_n(&mut ns, g, 0);

        let k = KuhnPoker::from_actions(&[0, 1]).istate_key(0);
        assert_eq!(
            ns.get_node_mut(&k).unwrap().get_average_strategy(),
            [1.0, 0.0, 0.0, 0.0, 0.0]
        );

        let k = KuhnPoker::from_actions(&[1, 0]).istate_key(0);
        assert_eq!(
            ns.get_node_mut(&k).unwrap().get_average_strategy(),
            [1.0, 0.0, 0.0, 0.0, 0.0]
        );

        let k = KuhnPoker::from_actions(&[0, 1, 0]).istate_key(0);
        assert_eq!(
            ns.get_node_mut(&k).unwrap().get_average_strategy(),
            [1.0, 0.0, 0.0, 0.0, 0.0]
        );

        let k = KuhnPoker::from_actions(&[0, 1, 1]).istate_key(0);
        assert_eq!(
            ns.get_node_mut(&k).unwrap().get_average_strategy(),
            [1.0, 0.0, 0.0, 0.0, 0.0]
        );
    }
}
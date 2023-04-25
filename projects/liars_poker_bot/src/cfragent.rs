pub mod bestresponse;
pub mod cfr;
pub mod cfrcs;
pub mod cfrnode;

use std::{iter::zip, marker::PhantomData};

use itertools::Itertools;
use log::{debug, info, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    agents::Agent,
    cfragent::{bestresponse::BestResponse, cfr::Algorithm, cfrcs::CFRCS},
    database::NodeStore,
    game::{Action, Game, GameState},
    istate::IStateKey,
};

use self::cfrnode::CFRNode;

pub struct CFRAgent<T: GameState, N: NodeStore<CFRNode>> {
    game: Game<T>,
    rng: StdRng,
    // store: FileNodeStore<FileBackend>,
    ns: N,
    _phantom: PhantomData<T>,
}

impl<T: GameState, N: NodeStore<CFRNode>> CFRAgent<T, N> {
    pub fn new(game: Game<T>, seed: u64, iterations: usize, ns: N) -> Self {
        let mut agent = Self {
            game: game.clone(),
            rng: SeedableRng::seed_from_u64(seed),
            // store: FileNodeStore::new(FileBackend::new(storage)),
            ns: ns,
            _phantom: PhantomData,
        };

        // Use CFR to train the agent
        let mut br = BestResponse::new();
        info!("Starting self play for CFR");
        let mut alg = CFRCS::new(seed);
        // let mut alg = VanillaCFR::new();
        for _ in 0..iterations {
            let gs = (agent.game.new)();

            for i in 0..agent.game.max_players {
                alg.run(&mut agent.ns, &gs, i);

                if alg.nodes_touched() % 10 == 0 {
                    info!(
                        "\t{}\t{}",
                        alg.nodes_touched(),
                        br.estimate_exploitability(&game, &mut agent.ns, 0, 5000)
                    )
                }
            }

            // info!("Finished iteration {} for CFR", i);
        }

        // Save the trained policy
        debug!("finished training policy");

        return agent;
    }

    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        self.ns.get_node_mut(istate)
    }

    fn get_policy(&mut self, istate: &IStateKey) -> Vec<f32> {
        let n = self.get_node_mut(istate).unwrap();
        let p = n.get_average_strategy();
        return p;
    }
}

impl<T: GameState, N: NodeStore<CFRNode>> Agent<T> for CFRAgent<T, N> {
    /// Chooses a random action weighted by the policy for the current istate.
    ///
    /// If the I state has not be
    fn step(&mut self, s: &T) -> Action {
        let istate = s.istate_key(s.cur_player());

        let p = self.get_policy(&istate);
        trace!("evaluating istate {} for {:?}", istate.to_string(), p);
        let mut weights = vec![0.0; s.legal_actions().len()];
        for &a in &s.legal_actions() {
            weights[a] = p[a];
        }
        return zip(s.legal_actions(), weights)
            .collect_vec()
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0;
    }
}

#[cfg(test)]
mod tests {
    use super::CFRAgent;
    use crate::{
        agents::Agent,
        database::memory_node_store::MemoryNodeStore,
        game::{
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
    };

    #[test]
    fn cfragent_sample_test() {
        let mut qa = CFRAgent::new(KuhnPoker::game(), 42, 50000, MemoryNodeStore::new());
        let mut s = KuhnPoker::new_state();
        s.apply_action(1);
        s.apply_action(0);
        s.apply_action(KPAction::Pass as usize);

        assert_eq!(s.istate_string(1), "0p");

        let mut action_counter = vec![0; 2];
        for _ in 0..1000 {
            let a = qa.step(&s);
            action_counter[a] += 1;
        }

        // For state 0p, should bet about 33% of the time in nash equilibrium
        assert!(action_counter[KPAction::Bet as usize] > 300);
        assert!(action_counter[KPAction::Bet as usize] < 400);
    }
}

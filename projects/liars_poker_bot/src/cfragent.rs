pub mod cfr;
pub mod cfrcs;
pub mod cfrnode;

use std::marker::PhantomData;

use clap::clap_derive::ArgEnum;
use log::{debug, info, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    actions,
    agents::Agent,
    algorithms::exploitability,
    cfragent::{
        cfr::{Algorithm, VanillaCFR},
        cfrcs::CFRCS,
    },
    database::NodeStore,
    game::{Action, Game, GameState},
    istate::IStateKey,
    policy::Policy,
};

use self::cfrnode::{ActionVec, CFRNode};

#[derive(ArgEnum, Clone, Copy, Debug)]
pub enum CFRAlgorithm {
    CFR,
    CFRCS,
}

pub struct CFRAgent<T: GameState, N: NodeStore<CFRNode>> {
    game: Game<T>,
    rng: StdRng,
    // store: FileNodeStore<FileBackend>,
    ns: N,
    _phantom: PhantomData<T>,
}

impl<T: GameState, N: NodeStore<CFRNode> + Policy<T>> CFRAgent<T, N> {
    pub fn new(game: Game<T>, seed: u64, iterations: usize, ns: N, alg: CFRAlgorithm) -> Self {
        let mut agent = Self {
            game: game.clone(),
            rng: SeedableRng::seed_from_u64(seed),
            // store: FileNodeStore::new(FileBackend::new(storage)),
            ns,
            _phantom: PhantomData,
        };

        match alg {
            CFRAlgorithm::CFR => train(&mut agent, game, iterations, &mut VanillaCFR::new()),
            CFRAlgorithm::CFRCS => train(&mut agent, game, iterations, &mut CFRCS::new(seed)),
        }

        agent
    }

    fn get_policy(&mut self, istate: &IStateKey) -> ActionVec<f64> {
        let n = self.ns.get(istate).unwrap();
        let p = n.borrow().get_average_strategy();
        self.ns.insert_node(*istate, n); // return the node
        p
    }
}

impl<T: GameState, N: NodeStore<CFRNode> + Policy<T>> Agent<T> for CFRAgent<T, N> {
    /// Chooses a random action weighted by the policy for the current istate.
    ///
    /// If the I state has not be
    fn step(&mut self, s: &T) -> Action {
        let istate = s.istate_key(s.cur_player());

        let p = self.get_policy(&istate);
        trace!("evaluating istate {} for {:?}", istate.to_string(), p);
        let mut weights = ActionVec::new(&actions!(s));
        for &a in &actions!(s) {
            weights[a] = p[a];
        }
        return weights
            .to_vec()
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0;
    }
}

fn train<T: GameState, N: NodeStore<CFRNode> + Policy<T>, A: Algorithm>(
    agent: &mut CFRAgent<T, N>,
    game: Game<T>,
    iterations: usize,
    alg: &mut A,
) {
    info!("Starting self play for CFR");
    let mut print_freq = 1;

    for iteration in 0..iterations {
        let gs = (agent.game.new)();

        for p in 0..agent.game.max_players {
            alg.run(&mut agent.ns, &gs, p);
        }

        if iteration % print_freq == 0 {
            debug!(
                "finished iteration: {}, starting best response calculation",
                iteration
            );
            let exploitability =
                exploitability::exploitability(game.clone(), &mut agent.ns).nash_conv;
            info!(
                "exploitability:\t{}\t{}\t{}",
                iteration,
                alg.nodes_touched(),
                exploitability
            );
            print_freq *= 2;
        }

        // info!("Finished iteration {} for CFR", i);
    }

    // Save the trained policy
    debug!("finished training policy");
}

#[cfg(test)]
mod tests {
    use super::CFRAgent;
    use crate::{
        actions,
        agents::Agent,
        cfragent::{cfrnode::ActionVec, CFRAlgorithm},
        database::memory_node_store::MemoryNodeStore,
        game::{
            kuhn_poker::{KPAction, KuhnPoker},
            GameState,
        },
    };

    #[test]
    fn cfragent_sample_test() {
        let mut qa = CFRAgent::new(
            KuhnPoker::game(),
            42,
            50000,
            MemoryNodeStore::new(),
            CFRAlgorithm::CFRCS,
        );
        let mut s = KuhnPoker::new_state();
        s.apply_action(KPAction::Queen.into());
        s.apply_action(KPAction::Jack.into());
        s.apply_action(KPAction::Pass.into());

        assert_eq!(s.istate_string(1), "Jackp");

        let mut action_counter: ActionVec<usize> = ActionVec::new(&actions!(s));
        for _ in 0..1000 {
            let a = qa.step(&s);
            action_counter[a] += 1;
        }

        // For state 0p, should bet about 33% of the time in nash equilibrium
        assert!(action_counter[KPAction::Bet.into()] > 320);
        assert!(action_counter[KPAction::Bet.into()] < 340);
    }
}

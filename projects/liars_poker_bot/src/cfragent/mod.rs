pub mod cfr;
pub mod cfrcs;
pub mod cfres;
pub mod cfrnode;

use clap::ValueEnum;
use log::{debug, trace};
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use crate::{
    actions,
    agents::Agent,
    cfragent::{
        cfr::{Algorithm, VanillaCFR},
        cfrcs::CFRCS,
    },
    database::NodeStore,
    game::{Action, GameState},
    istate::IStateKey,
    policy::Policy,
};

use self::cfrnode::{ActionVec, CFRNode};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CFRAlgorithm {
    CFR,
    CFRCS,
    CFRES,
}

pub struct CFRAgent<T: GameState, N: NodeStore<CFRNode>> {
    pub game_generator: fn() -> T,
    rng: StdRng,
    alg: CFRAlgorithm,
    nodes_touched: usize,
    // store: FileNodeStore<FileBackend>,
    pub ns: N,
}

impl<T: GameState, N: NodeStore<CFRNode> + Policy<T>> CFRAgent<T, N> {
    pub fn new(game_generator: fn() -> T, seed: u64, ns: N, alg: CFRAlgorithm) -> Self {
        Self {
            game_generator,
            rng: SeedableRng::seed_from_u64(seed),
            // store: FileNodeStore::new(FileBackend::new(storage)),
            ns,
            alg,
            nodes_touched: 0,
        }
    }

    fn get_policy(&mut self, istate: &IStateKey) -> ActionVec<f64> {
        let n = self.ns.get(istate).unwrap();
        let p = n.borrow().get_average_strategy();
        self.ns.insert_node(*istate, n); // return the node
        p
    }

    pub fn train(&mut self, iterations: usize) {
        let seed = self.rng.gen();
        let nodes_touched = match self.alg {
            CFRAlgorithm::CFR => train(self, iterations, &mut VanillaCFR::new()),
            CFRAlgorithm::CFRCS => train(self, iterations, &mut CFRCS::new(seed)),
            CFRAlgorithm::CFRES => todo!(),
        };

        self.nodes_touched += nodes_touched;
    }

    pub fn nodes_touched(&self) -> usize {
        self.nodes_touched
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

    fn get_name(&self) -> String {
        format!("CFR, nodes touched: {}", self.nodes_touched)
    }
}

fn train<T: GameState, N: NodeStore<CFRNode> + Policy<T>, A: Algorithm>(
    agent: &mut CFRAgent<T, N>,
    iterations: usize,
    alg: &mut A,
) -> usize {
    debug!("starting {} training iterations for CFR", iterations);
    for _ in 0..iterations {
        let gs = (agent.game_generator)();

        for p in 0..gs.num_players() {
            alg.run(&mut agent.ns, &gs, p);
        }
    }
    // Save the trained policy
    debug!("finished training for CFR");
    alg.nodes_touched()
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
            || (KuhnPoker::game().new)(),
            42,
            MemoryNodeStore::default(),
            CFRAlgorithm::CFRCS,
        );
        qa.train(50000);
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
